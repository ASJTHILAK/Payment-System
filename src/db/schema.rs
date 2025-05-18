use crate::models::{Account, Transaction, TransactionStatus, User};
use anyhow::Result;
use sqlx::SqlitePool;
use uuid::Uuid;

pub async fn create_user(
    pool: &SqlitePool,
    username: &str,
    email: &str,
    password_hash: &str,
) -> Result<User> {
    let id = Uuid::new_v4().to_string();
    let now = chrono::Utc::now().naive_utc();

    sqlx::query!(
        r#"
        INSERT INTO users (
            id, username, email, password_hash,
            created_at, updated_at
        )
        VALUES (?, ?, ?, ?, ?, ?)
        "#,
        id,
        username,
        email,
        password_hash,
        now,
        now
    )
    .execute(pool)
    .await?;

    // Fetch the inserted user
    let user = sqlx::query_as!(
        User,
        r#"
        SELECT 
            id as "id!", 
            username as "username!", 
            email as "email!", 
            password_hash as "password_hash!",
            created_at as "created_at!", 
            updated_at as "updated_at!"
        FROM users WHERE id = ?
        "#,
        id
    )
    .fetch_one(pool)
    .await?;

    Ok(user)
}

pub async fn get_user_by_username(pool: &SqlitePool, username: &str) -> Result<Option<User>> {
    let user = sqlx::query_as!(
        User,
        r#"
        SELECT 
            id as "id!", 
            username as "username!", 
            email as "email!", 
            password_hash as "password_hash!",
            created_at as "created_at!", 
            updated_at as "updated_at!"
        FROM users WHERE username = ?
        "#,
        username
    )
    .fetch_optional(pool)
    .await?;

    Ok(user)
}

pub async fn create_account(
    pool: &SqlitePool,
    user_id: &str,
    currency: &str,
    initial_balance: f64,
) -> Result<Account> {
    let now = chrono::Utc::now().naive_utc();

    sqlx::query!(
        r#"
        INSERT INTO accounts (id, currency, balance, created_at, updated_at)
        VALUES (?, ?, ?, ?, ?)
        "#,
        user_id, // Using user_id as the account id
        currency,
        initial_balance,
        now,
        now
    )
    .execute(pool)
    .await?;

    let account = sqlx::query_as!(
        Account,
        r#"
        SELECT 
            id as "id!", 
            balance as "balance!", 
            currency as "currency!",
            created_at as "created_at!", 
            updated_at as "updated_at!"
        FROM accounts WHERE id = ?
        "#,
        user_id // Using user_id instead of id since they're now the same
    )
    .fetch_one(pool)
    .await?;

    Ok(account)
}

pub async fn get_account_by_id(pool: &SqlitePool, account_id: &str) -> Result<Option<Account>> {
    let account = sqlx::query_as!(
        Account,
        r#"
        SELECT 
            id as "id!", 
            balance as "balance!", 
            currency as "currency!",
            created_at as "created_at!", 
            updated_at as "updated_at!"
        FROM accounts WHERE id = ?
        "#,
        account_id
    )
    .fetch_optional(pool)
    .await?;

    Ok(account)
}

pub async fn update_account_balance(
    pool: &SqlitePool,
    account_id: &str,
    new_balance: f64,
) -> Result<()> {
    let now = chrono::Utc::now().naive_utc();

    sqlx::query!(
        r#"
        UPDATE accounts
        SET balance = ?, updated_at = ?
        WHERE id = ?
        "#,
        new_balance,
        now,
        account_id
    )
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn get_user_transactions(
    pool: &SqlitePool,
    account_id: &str,
) -> Result<Vec<Transaction>> {
    let transactions = sqlx::query_as!(
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
            created_at as "created_at!", 
            updated_at as "updated_at!"
        FROM transactions
        WHERE from_account_id = ? OR to_account_id = ?
        ORDER BY created_at DESC
        "#,
        account_id,
        account_id
    )
    .fetch_all(pool)
    .await?;

    Ok(transactions)
}

pub async fn create_transaction(
    pool: &SqlitePool,
    from_account_id: &str,
    to_account_id: &str,
    amount: f64,
    currency: &str,
    description: Option<&str>,
) -> Result<Transaction> {
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
    .execute(pool)
    .await?;

    // Fetch the created transaction
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
            created_at as "created_at!", 
            updated_at as "updated_at!"
        FROM transactions 
        WHERE id = ?
        "#,
        id
    )
    .fetch_one(pool)
    .await?;

    Ok(transaction)
}

pub async fn update_transaction_status(
    pool: &SqlitePool,
    transaction_id: &str,
    status: TransactionStatus,
) -> Result<()> {
    let now = chrono::Utc::now().naive_utc();
    let status_str = status.to_string();

    sqlx::query!(
        r#"
        UPDATE transactions
        SET status = ?, updated_at = ?
        WHERE id = ?
        "#,
        status_str,
        now,
        transaction_id
    )
    .execute(pool)
    .await?;

    Ok(())
}
