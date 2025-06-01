use crate::{
    db::{DbError, DbResult},
    models::Account,
};
use std::sync::{Arc, Mutex};
use tracing::{debug, error};

/// Fetches an account from the database by its ID
pub fn get_account(conn: &Arc<Mutex<rusqlite::Connection>>, account_id: &str) -> DbResult<Account> {
    let conn = conn.lock().map_err(|e| {
        error!("Failed to acquire database lock: {}", e);
        DbError::from("Failed to acquire database lock")
    })?;

    let account = conn
        .query_row(
            "SELECT id, balance, currency, country, created_at, updated_at 
         FROM accounts 
         WHERE id = ?1",
            rusqlite::params![account_id],
            |row| {
                Ok(Account {
                    id: row.get(0)?,
                    balance: row.get(1)?,
                    currency: row.get(2)?,
                    country: row.get(3)?,
                    created_at: row.get(4)?,
                    updated_at: row.get(5)?,
                })
            },
        )
        .map_err(|e| {
            error!("Database error fetching account {}: {}", account_id, e);
            match e {
                rusqlite::Error::QueryReturnedNoRows => {
                    DbError::from(format!("Account not found: {}", account_id))
                }
                _ => DbError::from(e.to_string()),
            }
        })?;

    Ok(account)
}

/// Validates transaction parameters for both accounts
pub fn validate_transaction(
    conn: &Arc<Mutex<rusqlite::Connection>>,
    from_account_id: &str,
    to_account_id: &str,
    amount: f64,
    currency: &str,
) -> DbResult<(Account, Account)> {
    debug!(
        "Validating transaction: from={}, to={}, amount={}, currency={}",
        from_account_id, to_account_id, amount, currency
    );

    let from_account = get_account(conn, from_account_id)?;
    let to_account = get_account(conn, to_account_id)?;

    // Validate currency
    if from_account.currency != currency {
        return Err(DbError::from(format!(
            "Currency mismatch: sender account uses {} but transaction is in {}",
            from_account.currency, currency
        )));
    }

    // Check sufficient balance
    if from_account.balance < amount {
        return Err(DbError::from("Insufficient balance"));
    }

    Ok((from_account, to_account))
}

/// Updates account balances within a transaction
#[allow(dead_code)]
pub fn update_account_balance(
    tx: &rusqlite::Transaction,
    account_id: &str,
    new_balance: f64,
) -> DbResult<()> {
    tx.execute(
        "UPDATE accounts 
        SET balance = ?1, updated_at = CURRENT_TIMESTAMP 
        WHERE id = ?2",
        rusqlite::params![new_balance, account_id],
    )
    .map_err(|e| {
        error!("Failed to update account balance: {}", e);
        DbError::from(e.to_string())
    })?;

    Ok(())
}
