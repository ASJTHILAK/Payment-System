use sqlx::SqlitePool;
use tracing::{debug, error, info};
use uuid::Uuid;

use crate::{
    db::error::{DbError, DbResult},
    models::{Transaction, TransactionStatus},
};

/// This function handles the entire transaction process within a database transaction
/// to prevent race conditions and ensure data consistency.
pub async fn process_transaction(
    pool: &SqlitePool,
    from_account_id: &str,
    to_account_id: &str,
    amount: f64,
    currency: &str,
    description: Option<&str>,
) -> DbResult<Transaction> {
    debug!(
        "Starting atomic transaction process: from={}, to={}, amount={}, currency={}",
        from_account_id, to_account_id, amount, currency
    );

    // Validate accounts and transaction parameters
    let (from_account, to_account) =
        crate::db::validate_transaction(pool, from_account_id, to_account_id, amount, currency)
            .await?;

    // Additional validation for same-currency transactions
    if to_account.currency != currency {
        return Err(DbError::from(
            "Currency mismatch: recipient account currency must match transaction currency",
        ));
    }

    // Now that all validations are complete, start the database transaction
    let mut tx = pool.begin().await.map_err(|e| {
        error!("Failed to begin database transaction: {}", e);
        DbError::from(e)
    })?;

    // Create a transaction record
    let id = Uuid::new_v4().to_string();
    let now = chrono::Utc::now().naive_utc();
    let status = TransactionStatus::Pending.to_string();

    sqlx::query!(
        r#"
        INSERT INTO transactions (
            id, from_account_id, to_account_id,
            amount, currency, status, description,
            created_at, updated_at
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
        id,
        from_account_id,
        to_account_id,
        amount,
        currency,
        status,
        description,
        now,
        now
    )
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        error!("Database error creating transaction record: {}", e);
        DbError::from(e)
    })?;

    // Update sender balance - deduct the amount with optimistic concurrency check
    let now = chrono::Utc::now().naive_utc();
    let new_from_balance = from_account.balance - amount;

    // Ensure the balance hasn't changed since we read it (optimistic concurrency control)
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

    // Update recipient balance - add the amount with optimistic concurrency check
    let now = chrono::Utc::now().naive_utc();
    let new_to_balance = to_account.balance + amount;

    // Ensure the balance hasn't changed since we read it (optimistic concurrency control)
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

    // Check if the update affected a row (if not, the balance has changed since we read it)
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
        "Transaction completed successfully: id={}, from={}, to={}, amount={}, currency={}",
        transaction.id, from_account_id, to_account_id, amount, currency
    );

    Ok(transaction)
}
