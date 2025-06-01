use rusqlite::Connection;
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use tracing::{debug, error, info};
use uuid::Uuid;

use crate::{
    db::error::{DbError, DbResult},
    models::{Transaction, TransactionStatus},
    utils::currency_mapping::is_cross_border_by_currency,
};

/// This function handles the entire transaction process within a database transaction
/// to prevent race conditions and ensure data consistency.
pub fn process_transaction(
    conn: &Arc<Mutex<Connection>>,
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
        crate::db::validate_transaction(conn, from_account_id, to_account_id, amount, currency)?;

    // Additional validation for same-currency transactions
    if to_account.currency != currency {
        return Err(DbError::from(format!(
            "Currency mismatch: recipient account uses {} but transaction is in {}",
            to_account.currency, currency
        )));
    }

    // Determine if this is a cross-border transaction
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

    debug!("Transaction cross-border status: {}", is_cross_border);

    // Create a transaction record
    let id = Uuid::new_v4().to_string();
    let now = chrono::Utc::now().naive_utc();
    let status = TransactionStatus::Pending.to_string();

    let mut guard = conn.lock().map_err(|e| {
        error!("Failed to acquire database lock: {}", e);
        DbError::from("Database lock error")
    })?;

    // Start a database transaction
    let tx = guard.transaction().map_err(|e| {
        error!("Failed to start database transaction: {}", e);
        DbError::from(e)
    })?;

    // Insert transaction record
    tx.execute(
        r#"
        INSERT INTO transactions (
            id, from_account_id, to_account_id,
            amount, currency, status, description, is_cross_border,
            created_at, updated_at
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
        "#,
        rusqlite::params![
            id,
            from_account_id,
            to_account_id,
            amount,
            currency,
            status,
            description,
            Some(is_cross_border),
            now,
            now
        ],
    )
    .map_err(|e| {
        error!("Database error creating transaction record: {}", e);
        DbError::from(e)
    })?;

    // Update sender balance - deduct the amount with optimistic concurrency check
    let now = chrono::Utc::now().naive_utc();
    let new_from_balance = from_account.balance - amount;

    // Ensure the balance hasn't changed since we read it (optimistic concurrency control)
    let rows_affected = tx
        .execute(
            r#"
        UPDATE accounts
        SET balance = ?1, updated_at = ?2
        WHERE id = ?3 AND balance = ?4
        "#,
            rusqlite::params![new_from_balance, now, from_account_id, from_account.balance],
        )
        .map_err(|e| {
            error!("Database error updating sender balance: {}", e);
            DbError::from(e)
        })?;

    // Check if the update affected a row (if not, the balance has changed since we read it)
    if rows_affected == 0 {
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
    let rows_affected = tx
        .execute(
            r#"
        UPDATE accounts
        SET balance = ?1, updated_at = ?2
        WHERE id = ?3 AND balance = ?4
        "#,
            rusqlite::params![new_to_balance, now, to_account_id, to_account.balance],
        )
        .map_err(|e| {
            error!("Database error updating recipient balance: {}", e);
            DbError::from(e)
        })?;

    // Check if the update affected a row (if not, the balance has changed since we read it)
    if rows_affected == 0 {
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
        r#"
        UPDATE transactions
        SET status = ?1, updated_at = ?2
        WHERE id = ?3
        "#,
        rusqlite::params![completed_status, now, id],
    )
    .map_err(|e| {
        error!("Database error updating transaction status: {}", e);
        DbError::from(e)
    })?;

    // Commit the transaction
    tx.commit().map_err(|e| {
        error!("Failed to commit transaction: {}", e);
        DbError::from(e)
    })?;

    // Fetch the completed transaction to return
    let mut stmt = guard
        .prepare(
            r#"
        SELECT 
            id, from_account_id, to_account_id,
            amount, currency, status, description,
            exchange_rate, original_amount, original_currency, is_cross_border,
            created_at, updated_at
        FROM transactions 
        WHERE id = ?1
        "#,
        )
        .map_err(|e| {
            error!("Database error preparing fetch statement: {}", e);
            DbError::from(e)
        })?;

    let transaction = stmt
        .query_row(rusqlite::params![id], |row| {
            Ok(Transaction {
                id: row.get(0)?,
                from_account_id: row.get(1)?,
                to_account_id: row.get(2)?,
                amount: row.get(3)?,
                currency: row.get(4)?,
                status: TransactionStatus::from_str(&row.get::<_, String>(5)?)
                    .unwrap_or(TransactionStatus::Pending),
                description: row.get(6)?,
                exchange_rate: row.get(7)?,
                original_amount: row.get(8)?,
                original_currency: row.get(9)?,
                is_cross_border: row.get::<_, Option<bool>>(10)?,
                created_at: row.get(11)?,
                updated_at: row.get(12)?,
            })
        })
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
