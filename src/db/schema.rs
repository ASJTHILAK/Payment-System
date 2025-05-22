use crate::db::error::{DbError, DbResult};
use crate::models::{Account, Transaction, TransactionStatus, User};
use sqlx::SqlitePool;
use tracing::{debug, error, info};
use uuid::Uuid;

pub async fn create_user(
    pool: &SqlitePool,
    username: &str,
    email: &str,
    password_hash: &str,
) -> DbResult<User> {
    debug!("Creating new user: username={}, email={}", username, email);

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
    .await
    .map_err(DbError::from)?;

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
    .await
    .map_err(|e| {
        error!("Database error fetching newly created user {}: {}", id, e);
        DbError::from(e)
    })?;

    info!(
        "User created successfully: id={}, username={}",
        id, username
    );
    Ok(user)
}

pub async fn get_user_by_username(pool: &SqlitePool, username: &str) -> DbResult<Option<User>> {
    debug!("Fetching user by username: {}", username);

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
    .await
    .map_err(|e| {
        error!(
            "Database error fetching user by username {}: {}",
            username, e
        );
        DbError::from(e)
    })?;

    if let Some(ref user) = user {
        debug!("User found: id={}, username={}", user.id, user.username);
    } else {
        debug!("No user found with username: {}", username);
    }

    Ok(user)
}

pub async fn create_account(
    pool: &SqlitePool,
    user_id: &str,
    currency: &str,
    initial_balance: f64,
) -> DbResult<Account> {
    debug!(
        "Creating new account: user_id={}, currency={}, balance={}",
        user_id, currency, initial_balance
    );

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
    .await
    .map_err(|e| {
        error!(
            "Database error creating account for user {}: {}",
            user_id, e
        );
        DbError::from(e)
    })?;

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
    .await
    .map_err(|e| {
        error!(
            "Database error fetching newly created account for user {}: {}",
            user_id, e
        );
        DbError::from(e)
    })?;

    info!(
        "Account created successfully: user_id={}, currency={}, balance={}",
        user_id, currency, initial_balance
    );
    Ok(account)
}

pub async fn get_account_by_id(pool: &SqlitePool, account_id: &str) -> DbResult<Option<Account>> {
    debug!("Fetching account by id: {}", account_id);

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
    .await
    .map_err(|e| {
        error!("Database error fetching account {}: {}", account_id, e);
        DbError::from(e)
    })?;

    if let Some(ref acc) = account {
        debug!(
            "Account found: id={}, balance={}, currency={}",
            acc.id, acc.balance, acc.currency
        );
    } else {
        debug!("No account found with id: {}", account_id);
    }

    Ok(account)
}

pub async fn get_user_transactions(
    pool: &SqlitePool,
    account_id: &str,
) -> DbResult<Vec<Transaction>> {
    debug!("Fetching transactions for account: {}", account_id);

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
    .await
    .map_err(|e| {
        error!(
            "Database error fetching transactions for account {}: {}",
            account_id, e
        );
        DbError::from(e)
    })?;

    info!(
        "Retrieved {} transactions for account {}",
        transactions.len(),
        account_id
    );
    Ok(transactions)
}
