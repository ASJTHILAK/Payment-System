use sqlx::SqlitePool;
use tracing::{debug, error, info};
use uuid::Uuid;

use crate::{
    db::{DbError, DbResult},
    models::{Account, Transaction, TransactionStatus},
    services::{
        compliance::{ComplianceService, ComplianceStatus},
        exchange_rate::ExchangeRateService,
    },
};

/// Process a cross-border transaction with currency conversion if needed
pub async fn process_cross_border_transaction(
    pool: &SqlitePool,
    exchange_rate_service: &ExchangeRateService,
    compliance_service: &ComplianceService,
    from_account_id: &str,
    to_account_id: &str,
    amount: f64,
    currency: &str,
    convert_currency: bool,
    description: Option<&str>,
) -> DbResult<Transaction> {
    debug!(
        "Starting cross-border transaction: from={}, to={}, amount={}, currency={}, convert={}",
        from_account_id, to_account_id, amount, currency, convert_currency
    );

    // First get accounts before starting transaction
    // Get sender's account
    let from_account = sqlx::query_as!(
        Account,
        r#"
        SELECT 
            id as "id!", 
            balance as "balance!", 
            currency as "currency!",
            country,
            created_at as "created_at!", 
            updated_at as "updated_at!"
        FROM accounts 
        WHERE id = ?
        "#,
        from_account_id
    )
    .fetch_optional(pool)
    .await
    .map_err(|e| {
        error!(
            "Database error fetching sender account {}: {}",
            from_account_id, e
        );
        DbError::from(e)
    })?
    .ok_or_else(|| {
        error!("Sender account not found: {}", from_account_id);
        DbError::from("Sender account not found")
    })?;

    // Get recipient's account
    let to_account = sqlx::query_as!(
        Account,
        r#"
        SELECT 
            id as "id!", 
            balance as "balance!", 
            currency as "currency!",
            country,
            created_at as "created_at!", 
            updated_at as "updated_at!"
        FROM accounts 
        WHERE id = ?
        "#,
        to_account_id
    )
    .fetch_optional(pool)
    .await
    .map_err(|e| {
        error!(
            "Database error fetching recipient account {}: {}",
            to_account_id, e
        );
        DbError::from(e)
    })?
    .ok_or_else(|| {
        error!("Recipient account not found: {}", to_account_id);
        DbError::from("Recipient account not found")
    })?;

    // Check if the sender has sufficient funds in their currency
    if from_account.currency != currency {
        return Err(DbError::from(format!(
            "Currency mismatch: account uses {} but transaction is in {}",
            from_account.currency, currency
        )));
    }

    // Check if sender has sufficient balance
    if from_account.balance < amount {
        return Err(DbError::from("Insufficient balance"));
    }

    // Determine if this is a cross-border transaction
    let is_cross_border = match (
        from_account.country.as_deref(),
        to_account.country.as_deref(),
    ) {
        (Some(from_country), Some(to_country)) => from_country != to_country,
        _ => false, // Default to false if country information is missing
    };

    // Variables to track original and converted amounts
    let mut final_amount = amount;
    let mut exchange_rate: Option<f64> = None;
    let mut original_amount: Option<f64> = None;
    let mut original_currency: Option<String> = None;
    let target_currency = to_account.currency.clone();

    // Perform currency conversion if needed and requested
    if convert_currency && from_account.currency != to_account.currency {
        debug!(
            "Currency conversion needed: {} to {}",
            from_account.currency, to_account.currency
        );

        // Convert the amount to recipient's currency
        let (converted, rate) = match exchange_rate_service
            .convert_currency(amount, &from_account.currency, &to_account.currency)
            .await
        {
            Ok(result) => result,
            Err(e) => {
                error!("Currency conversion failed: {}", e);
                return Err(e);
            }
        };

        // Store original and converted values
        final_amount = converted;
        exchange_rate = Some(rate);
        original_amount = Some(amount);
        original_currency = Some(from_account.currency.clone());

        debug!(
            "Converted {} {} to {} {} (rate: {})",
            amount, from_account.currency, converted, to_account.currency, rate
        );
    } else if to_account.currency != currency {
        // If conversion isn't requested but currencies don't match, reject
        return Err(DbError::from(format!(
            "Currency mismatch: recipient account uses {} but transaction is in {}. Enable currency conversion to proceed.",
            to_account.currency, currency
        )));
    }

    // For cross-border transactions, perform compliance checks before starting transaction
    let compliance_status = if is_cross_border {
        // Create a transaction ID early for compliance checks
        let transaction_id = Uuid::new_v4().to_string();

        // Run compliance checks
        let from_country = from_account.country.as_deref().unwrap_or("UNKNOWN");
        let to_country = to_account.country.as_deref().unwrap_or("UNKNOWN");

        let compliance_result = match compliance_service
            .check_compliance(&transaction_id, from_country, to_country, amount, currency)
            .await
        {
            Ok(result) => result,
            Err(e) => {
                error!("Compliance check failed: {}", e);
                return Err(e);
            }
        };

        // Handle compliance status
        match compliance_result.status {
            ComplianceStatus::Approved => {
                debug!("Compliance check approved");
                compliance_result.status
            }
            ComplianceStatus::PendingReview => {
                debug!("Compliance check pending review");
                compliance_result.status
            }
            ComplianceStatus::Rejected => {
                error!("Compliance check rejected transaction");
                return Err(DbError::from(format!(
                    "Transaction rejected due to compliance issues: {}",
                    compliance_result
                        .details
                        .unwrap_or_else(|| "No details provided".to_string())
                )));
            }
        }
    } else {
        // Not cross-border, no compliance check needed
        ComplianceStatus::Approved
    };

    // Now that all external operations are complete, start the database transaction
    let mut tx = pool.begin().await.map_err(|e| {
        error!("Failed to begin database transaction: {}", e);
        DbError::from(e)
    })?;

    // Create a transaction record
    let id = Uuid::new_v4().to_string();
    let now = chrono::Utc::now().naive_utc();

    // Determine initial status based on compliance
    let initial_status = match compliance_status {
        ComplianceStatus::Approved => TransactionStatus::Pending,
        ComplianceStatus::PendingReview => TransactionStatus::Pending,
        ComplianceStatus::Rejected => TransactionStatus::Failed, // This shouldn't happen as we return early above
    };

    let status_str = initial_status.to_string();

    sqlx::query!(
        r#"
        INSERT INTO transactions (
            id, from_account_id, to_account_id,
            amount, currency, status, description,
            exchange_rate, original_amount, original_currency, is_cross_border,
            created_at, updated_at
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
        id,
        from_account_id,
        to_account_id,
        final_amount,
        target_currency,
        status_str,
        description,
        exchange_rate,
        original_amount,
        original_currency,
        is_cross_border,
        now,
        now
    )
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        error!("Database error creating transaction record: {}", e);
        DbError::from(e)
    })?;

    // If compliance status is pending review, don't proceed with the transfer yet
    if matches!(compliance_status, ComplianceStatus::PendingReview) {
        tx.commit().await.map_err(|e| {
            error!("Failed to commit transaction: {}", e);
            DbError::from(e)
        })?;

        // Fetch the pending transaction to return
        let transaction = sqlx::query_as!(
            Transaction,
            r#"
            SELECT 
                id as "id!", 
                from_account_id as "from_account_id!", 
                to_account_id as "to_account_id!",
                amount as "amount!", 
                currency as "currency!",
                status as "status: TransactionStatus",
                description,
                exchange_rate, original_amount, original_currency, is_cross_border,
                created_at as "created_at!", 
                updated_at as "updated_at!"
            FROM transactions 
            WHERE id = ?
            "#,
            id
        )
        .fetch_one(pool)
        .await
        .map_err(|e| {
            error!("Database error fetching transaction {}: {}", id, e);
            DbError::from(e)
        })?;

        info!(
            "Cross-border transaction pending compliance review: id={}",
            transaction.id
        );

        return Ok(transaction);
    }

    // Update sender balance - deduct the amount with optimistic concurrency check
    let now = chrono::Utc::now().naive_utc();
    let new_from_balance = from_account.balance - amount; // Always deduct the original amount

    // Ensure the balance hasn't changed since we read it
    let result = sqlx::query!(
        r#"
        UPDATE accounts
        SET balance = ?, updated_at = ?
        WHERE id = ? AND balance = ?
        "#,
        new_from_balance,
        now,
        from_account_id,
        from_account.balance
    )
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        error!("Database error updating sender balance: {}", e);
        DbError::from(e)
    })?;

    // Check if the update affected a row (if not, the balance has changed since we read it)
    if result.rows_affected() == 0 {
        tx.rollback().await.map_err(DbError::from)?;
        error!(
            "Concurrent update detected on sender account: {}",
            from_account_id
        );
        return Err(DbError::from(
            "Account balance changed during transaction. Please try again.",
        ));
    }

    debug!(
        "Updated sender balance: user={}, new_balance={}",
        from_account_id, new_from_balance
    );

    // Update recipient balance - add the converted amount with optimistic concurrency check
    let now = chrono::Utc::now().naive_utc();
    let new_to_balance = to_account.balance + final_amount; // Add the possibly converted amount

    // Ensure the balance hasn't changed since we read it
    let result = sqlx::query!(
        r#"
        UPDATE accounts
        SET balance = ?, updated_at = ?
        WHERE id = ? AND balance = ?
        "#,
        new_to_balance,
        now,
        to_account_id,
        to_account.balance
    )
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        error!("Database error updating recipient balance: {}", e);
        DbError::from(e)
    })?;

    // Check if the update affected a row
    if result.rows_affected() == 0 {
        tx.rollback().await.map_err(DbError::from)?;
        error!(
            "Concurrent update detected on recipient account: {}",
            to_account_id
        );
        return Err(DbError::from(
            "Account balance changed during transaction. Please try again.",
        ));
    }

    debug!(
        "Updated recipient balance: user={}, new_balance={}",
        to_account_id, new_to_balance
    );

    // Update transaction status to completed
    let now = chrono::Utc::now().naive_utc();
    let completed_status = TransactionStatus::Completed.to_string();

    sqlx::query!(
        r#"
        UPDATE transactions
        SET status = ?, updated_at = ?
        WHERE id = ?
        "#,
        completed_status,
        now,
        id
    )
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        error!("Database error updating transaction status: {}", e);
        DbError::from(e)
    })?;

    // Commit the transaction
    tx.commit().await.map_err(|e| {
        error!("Failed to commit transaction: {}", e);
        DbError::from(e)
    })?;

    // Fetch the completed transaction to return
    let transaction = sqlx::query_as!(
        Transaction,
        r#"
        SELECT 
            id as "id!", 
            from_account_id as "from_account_id!", 
            to_account_id as "to_account_id!",
            amount as "amount!", 
            currency as "currency!",
            status as "status: TransactionStatus",
            description,
            exchange_rate, original_amount, original_currency, is_cross_border,
            created_at as "created_at!", 
            updated_at as "updated_at!"
        FROM transactions 
        WHERE id = ?
        "#,
        id
    )
    .fetch_one(pool)
    .await
    .map_err(|e| {
        error!("Database error fetching transaction {}: {}", id, e);
        DbError::from(e)
    })?;

    info!(
        "Cross-border transaction completed successfully: id={}, from={}, to={}, amount={}, currency={}, converted={}",
        transaction.id, from_account_id, to_account_id, amount, currency,
        exchange_rate.is_some()
    );

    Ok(transaction)
}
