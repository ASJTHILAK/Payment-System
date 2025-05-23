use crate::{
    db::{DbError, DbResult},
    models::Account,
};
use sqlx::SqlitePool;
use tracing::{debug, error};

/// Fetches an account from the database by its ID
pub async fn get_account(pool: &SqlitePool, account_id: &str) -> DbResult<Account> {
    sqlx::query_as!(
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
        account_id
    )
    .fetch_optional(pool)
    .await
    .map_err(|e| {
        error!("Database error fetching account {}: {}", account_id, e);
        DbError::from(e)
    })?
    .ok_or_else(|| {
        error!("Account not found: {}", account_id);
        DbError::from(format!("Account not found: {}", account_id))
    })
}

/// Validates transaction parameters for both accounts
pub async fn validate_transaction(
    pool: &SqlitePool,
    from_account_id: &str,
    to_account_id: &str,
    amount: f64,
    currency: &str,
) -> DbResult<(Account, Account)> {
    debug!(
        "Validating transaction: from={}, to={}, amount={}, currency={}",
        from_account_id, to_account_id, amount, currency
    );

    let from_account = get_account(pool, from_account_id).await?;
    let to_account = get_account(pool, to_account_id).await?;

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
pub async fn update_account_balance(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    account_id: &str,
    new_balance: f64,
) -> DbResult<()> {
    sqlx::query!(
        r#"
        UPDATE accounts 
        SET balance = ?, updated_at = CURRENT_TIMESTAMP 
        WHERE id = ?
        "#,
        new_balance,
        account_id
    )
    .execute(&mut **tx)
    .await
    .map_err(|e| {
        error!("Failed to update account balance: {}", e);
        DbError::from(e)
    })?;

    Ok(())
}
