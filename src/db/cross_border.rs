use std::sync::{Arc, Mutex};
use tracing::{debug, error, info};
use uuid::Uuid;

use crate::{
    db::{DbError, DbResult},
    models::{Transaction, TransactionStatus},
    services::{
        compliance::{ComplianceService, ComplianceStatus},
        exchange_rate::ExchangeRateService,
    },
    utils::currency_mapping::is_cross_border_by_currency,
};

/// Process a cross-border transaction with currency conversion if needed
pub fn process_cross_border_transaction(
    db_conn: &Arc<Mutex<rusqlite::Connection>>,
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

    // Validate accounts and transaction parameters
    let (from_account, to_account) =
        crate::db::validate_transaction(db_conn, from_account_id, to_account_id, amount, currency)?;

    // For cross-border transactions, we don't immediately check recipient currency
    // as it might need conversion
    if !convert_currency && to_account.currency != currency {
        return Err(DbError::from(format!(
            "Currency mismatch: recipient account uses {} but transaction is in {}",
            to_account.currency, currency
        )));
    }

    // Additional validation for cross-border compliance
    if from_account.balance < amount {
        return Err(DbError::from("Insufficient balance"));
    }

    // Determine if this is a cross-border transaction
    // First try to use country information if available
    let is_cross_border = match (
        from_account.country.as_deref(),
        to_account.country.as_deref(),
    ) {
        (Some(from_country), Some(to_country)) => from_country != to_country,
        // If country information is missing, use currency-based detection
        _ => {
            debug!(
                "Country information missing, using currency-based cross-border detection: {} -> {}",
                from_account.currency, to_account.currency
            );
            is_cross_border_by_currency(&from_account.currency, &to_account.currency)
        }
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
        let (converted, rate) = match exchange_rate_service.convert_currency(
            amount,
            &from_account.currency,
            &to_account.currency,
        ) {
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

    let mut conn = db_conn.lock().map_err(|e| {
        error!("Failed to acquire database lock: {}", e);
        DbError::from("Failed to acquire database lock")
    })?;

    // Begin a transaction
    let tx = conn.transaction().map_err(|e| {
        error!("Failed to begin database transaction: {}", e);
        DbError::from(e.to_string())
    })?;

    // Create a transaction record first
    let id = Uuid::new_v4().to_string();
    let now = chrono::Utc::now().naive_utc();
    let initial_status = TransactionStatus::Pending.to_string();

    tx.execute(
        "INSERT INTO transactions (
            id, from_account_id, to_account_id,
            amount, currency, status, description,
            exchange_rate, original_amount, original_currency, is_cross_border,
            created_at, updated_at
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
        rusqlite::params![
            id,
            from_account_id,
            to_account_id,
            final_amount,
            target_currency,
            initial_status,
            description,
            exchange_rate,
            original_amount,
            original_currency,
            Some(is_cross_border),
            now,
            now
        ],
    )
    .map_err(|e| {
        error!("Database error creating transaction record: {}", e);
        DbError::from(e.to_string())
    })?;

    // Commit the transaction record first so it exists for foreign key constraints
    tx.commit().map_err(|e| {
        error!("Failed to commit initial transaction: {}", e);
        DbError::from(e.to_string())
    })?;

    // For cross-border transactions, perform compliance checks after transaction record exists
    if is_cross_border {
        // Run compliance checks with the existing connection to avoid deadlock
        let from_country = from_account.country.as_deref().unwrap_or("UNKNOWN");
        let to_country = to_account.country.as_deref().unwrap_or("UNKNOWN");

        let compliance_result = match compliance_service.check_compliance_with_conn(
            &conn,
            &id,
            from_country,
            to_country,
            final_amount,
            &target_currency,
        ) {
            Ok(result) => result,
            Err(e) => {
                error!("Compliance check failed: {}", e);
                // Update transaction status to failed and return
                let failed_status = TransactionStatus::Failed.to_string();
                let now = chrono::Utc::now().naive_utc();
                let _ = conn.execute(
                    "UPDATE transactions SET status = ?1, updated_at = ?2 WHERE id = ?3",
                    rusqlite::params![failed_status, now, id],
                );
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
                // Transaction remains pending - fetch and return it
                let transaction = conn
                    .query_row(
                        "SELECT id, from_account_id, to_account_id, amount, currency, 
                            status, description, exchange_rate, original_amount, 
                            original_currency, is_cross_border, created_at, updated_at
                     FROM transactions 
                     WHERE id = ?1",
                        rusqlite::params![id],
                        |row| {
                            let status_str: String = row.get(5)?;
                            let status = match status_str.as_str() {
                                "PENDING" => TransactionStatus::Pending,
                                "COMPLETED" => TransactionStatus::Completed,
                                "FAILED" => TransactionStatus::Failed,
                                _ => TransactionStatus::Failed,
                            };

                            Ok(Transaction {
                                id: row.get(0)?,
                                from_account_id: row.get(1)?,
                                to_account_id: row.get(2)?,
                                amount: row.get(3)?,
                                currency: row.get(4)?,
                                status,
                                description: row.get(6)?,
                                exchange_rate: row.get(7)?,
                                original_amount: row.get(8)?,
                                original_currency: row.get(9)?,
                                is_cross_border: Some(row.get::<_, i64>(10)? != 0),
                                created_at: row.get(11)?,
                                updated_at: row.get(12)?,
                            })
                        },
                    )
                    .map_err(|e| {
                        error!("Database error fetching transaction {}: {}", id, e);
                        DbError::from(e.to_string())
                    })?;

                info!(
                    "Cross-border transaction pending compliance review: id={}",
                    transaction.id
                );

                return Ok(transaction);
            }
            ComplianceStatus::Rejected => {
                error!("Compliance check rejected transaction");
                // Update transaction status to failed
                let failed_status = TransactionStatus::Failed.to_string();
                let now = chrono::Utc::now().naive_utc();
                let _ = conn.execute(
                    "UPDATE transactions SET status = ?1, updated_at = ?2 WHERE id = ?3",
                    rusqlite::params![failed_status, now, id],
                );
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

    // Now proceed with balance updates since compliance is approved
    let tx = conn.transaction().map_err(|e| {
        error!("Failed to begin balance update transaction: {}", e);
        DbError::from(e.to_string())
    })?;

    // Update sender balance - deduct the amount with optimistic concurrency check
    let now = chrono::Utc::now().naive_utc();
    let new_from_balance = from_account.balance - amount; // Always deduct the original amount

    // Ensure the balance hasn't changed since we read it
    let result = tx
        .execute(
            "UPDATE accounts
        SET balance = ?1, updated_at = ?2
        WHERE id = ?3 AND balance = ?4",
            rusqlite::params![new_from_balance, now, from_account_id, from_account.balance],
        )
        .map_err(|e| {
            error!("Database error updating sender balance: {}", e);
            DbError::from(e.to_string())
        })?;

    // Check if the update affected a row (if not, the balance has changed since we read it)
    if result == 0 {
        tx.rollback().map_err(|e| DbError::from(e.to_string()))?;
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
    let result = tx
        .execute(
            "UPDATE accounts
        SET balance = ?1, updated_at = ?2
        WHERE id = ?3 AND balance = ?4",
            rusqlite::params![new_to_balance, now, to_account_id, to_account.balance],
        )
        .map_err(|e| {
            error!("Database error updating recipient balance: {}", e);
            DbError::from(e.to_string())
        })?;

    // Check if the update affected a row
    if result == 0 {
        tx.rollback().map_err(|e| DbError::from(e.to_string()))?;
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

    tx.execute(
        "UPDATE transactions
        SET status = ?1, updated_at = ?2
        WHERE id = ?3",
        rusqlite::params![completed_status, now, id],
    )
    .map_err(|e| {
        error!("Database error updating transaction status: {}", e);
        DbError::from(e.to_string())
    })?;

    // Commit the transaction
    tx.commit().map_err(|e| {
        error!("Failed to commit transaction: {}", e);
        DbError::from(e.to_string())
    })?;

    // Fetch the completed transaction to return
    let transaction = conn
        .query_row(
            "SELECT id, from_account_id, to_account_id, amount, currency, 
                status, description, exchange_rate, original_amount, 
                original_currency, is_cross_border, created_at, updated_at
         FROM transactions 
         WHERE id = ?1",
            rusqlite::params![id],
            |row| {
                let status_str: String = row.get(5)?;
                let status = match status_str.as_str() {
                    "PENDING" => TransactionStatus::Pending,
                    "COMPLETED" => TransactionStatus::Completed,
                    "FAILED" => TransactionStatus::Failed,
                    _ => TransactionStatus::Failed,
                };

                Ok(Transaction {
                    id: row.get(0)?,
                    from_account_id: row.get(1)?,
                    to_account_id: row.get(2)?,
                    amount: row.get(3)?,
                    currency: row.get(4)?,
                    status,
                    description: row.get(6)?,
                    exchange_rate: row.get(7)?,
                    original_amount: row.get(8)?,
                    original_currency: row.get(9)?,
                    is_cross_border: Some(row.get::<_, i64>(10)? != 0),
                    created_at: row.get(11)?,
                    updated_at: row.get(12)?,
                })
            },
        )
        .map_err(|e| {
            error!("Database error fetching transaction {}: {}", id, e);
            DbError::from(e.to_string())
        })?;

    info!(
        "Cross-border transaction completed successfully: id={}, from={}, to={}, amount={}, currency={}, converted={}",
        transaction.id, from_account_id, to_account_id, amount, currency,
        exchange_rate.is_some()
    );

    Ok(transaction)
}
