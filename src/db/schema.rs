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

pub async fn update_account_balance(
    pool: &SqlitePool,
    account_id: &str,
    new_balance: f64,
) -> DbResult<()> {
    debug!(
        "Updating account balance: account_id={}, new_balance={}",
        account_id, new_balance
    );

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
    .await
    .map_err(|e| {
        error!(
            "Database error updating balance for account {}: {}",
            account_id, e
        );
        DbError::from(e)
    })?;

    info!(
        "Account balance updated: account_id={}, new_balance={}",
        account_id, new_balance
    );
    Ok(())
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

pub async fn create_transaction(
    pool: &SqlitePool,
    from_account_id: &str,
    to_account_id: &str,
    amount: f64,
    currency: &str,
    description: Option<&str>,
) -> DbResult<Transaction> {
    debug!(
        "Creating transaction: from={}, to={}, amount={}, currency={}, description={:?}",
        from_account_id, to_account_id, amount, currency, description
    );

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
    .await
    .map_err(|e| {
        error!("Database error creating transaction: {}", e);
        DbError::from(e)
    })?;

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
    .await
    .map_err(|e| {
        error!(
            "Database error fetching newly created transaction {}: {}",
            id, e
        );
        DbError::from(e)
    })?;

    info!(
        "Transaction created: id={}, from={}, to={}, amount={}, currency={}, status={:?}",
        transaction.id,
        transaction.from_account_id,
        transaction.to_account_id,
        transaction.amount,
        transaction.currency,
        transaction.status
    );
    Ok(transaction)
}

pub async fn update_transaction_status(
    pool: &SqlitePool,
    transaction_id: &str,
    status: TransactionStatus,
) -> DbResult<()> {
    debug!(
        "Updating transaction status: id={}, new_status={:?}",
        transaction_id, status
    );

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
    .await
    .map_err(|e| {
        error!(
            "Database error updating transaction status for {}: {}",
            transaction_id, e
        );
        DbError::from(e)
    })?;

    info!(
        "Transaction status updated: id={}, status={:?}",
        transaction_id, status
    );
    Ok(())
}
