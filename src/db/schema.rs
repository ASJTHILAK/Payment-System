use crate::db::error::{DbError, DbResult};
use crate::models::{Account, Transaction, TransactionStatus, User};
use crate::utils::currency_mapping::get_country_for_currency;
use rusqlite::OptionalExtension;
use std::sync::{Arc, Mutex};
use tracing::{debug, error, info};
use uuid::Uuid;

/// Create database schema for rusqlite connections
pub fn create_schema(conn: &rusqlite::Connection) -> rusqlite::Result<()> {
    // Create users table
    conn.execute(
        "CREATE TABLE IF NOT EXISTS users (
            id TEXT PRIMARY KEY,
            username TEXT NOT NULL UNIQUE,
            email TEXT NOT NULL UNIQUE,
            password_hash TEXT NOT NULL,
            created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
            updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
        )",
        [],
    )?;

    // Create accounts table with country support for cross-border payments
    conn.execute(
        "CREATE TABLE IF NOT EXISTS accounts (
            id TEXT PRIMARY KEY,
            balance REAL NOT NULL DEFAULT 0.0,
            currency TEXT NOT NULL DEFAULT 'INR',
            country TEXT DEFAULT 'IN',
            created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
            updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
            FOREIGN KEY (id) REFERENCES users(id)
        )",
        [],
    )?;

    // Create transactions table with cross-border payment support
    conn.execute(
        "CREATE TABLE IF NOT EXISTS transactions (
            id TEXT PRIMARY KEY,
            from_account_id TEXT NOT NULL,
            to_account_id TEXT NOT NULL,
            amount REAL NOT NULL,
            currency TEXT NOT NULL,
            status TEXT NOT NULL,
            description TEXT,
            exchange_rate REAL DEFAULT NULL,
            original_amount REAL DEFAULT NULL,
            original_currency TEXT DEFAULT NULL,
            is_cross_border BOOLEAN DEFAULT 0,
            created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
            updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
            FOREIGN KEY (from_account_id) REFERENCES accounts(id),
            FOREIGN KEY (to_account_id) REFERENCES accounts(id)
        )",
        [],
    )?;

    // Create exchange rates table to cache currency rates
    conn.execute(
        "CREATE TABLE IF NOT EXISTS exchange_rates (
            base_currency TEXT NOT NULL,
            target_currency TEXT NOT NULL,
            rate REAL NOT NULL,
            last_updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
            PRIMARY KEY (base_currency, target_currency)
        )",
        [],
    )?;

    // Create compliance checks table for cross-border transactions
    conn.execute(
        "CREATE TABLE IF NOT EXISTS compliance_checks (
            id TEXT PRIMARY KEY,
            transaction_id TEXT NOT NULL,
            source_country TEXT NOT NULL,
            destination_country TEXT NOT NULL,
            amount REAL NOT NULL,
            currency TEXT NOT NULL,
            risk_score REAL NOT NULL,
            status TEXT NOT NULL,
            details TEXT,
            created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
            updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
            FOREIGN KEY (transaction_id) REFERENCES transactions(id)
        )",
        [],
    )?;

    // Create indices for improved query performance
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_transactions_from_account ON transactions(from_account_id)",
        [],
    )?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_transactions_to_account ON transactions(to_account_id)",
        [],
    )?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_compliance_transaction ON compliance_checks(transaction_id)",
        [],
    )?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_exchange_rates ON exchange_rates(base_currency, target_currency)",
        [],
    )?;

    Ok(())
}

pub fn create_user(
    conn: &Arc<Mutex<rusqlite::Connection>>,
    username: &str,
    email: &str,
    password_hash: &str,
) -> DbResult<User> {
    debug!("Creating new user: username={}, email={}", username, email);

    let id = Uuid::new_v4().to_string();
    let now = chrono::Utc::now().naive_utc();

    let conn = conn.lock().map_err(|e| {
        error!("Failed to acquire database lock: {}", e);
        DbError::from("Failed to acquire database lock")
    })?;

    conn.execute(
        "INSERT INTO users (
            id, username, email, password_hash,
            created_at, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![id, username, email, password_hash, now, now],
    )
    .map_err(|e| {
        error!("Database error creating user: {}", e);
        DbError::from(e.to_string())
    })?;

    // Fetch the inserted user
    let user = conn
        .query_row(
            "SELECT id, username, email, password_hash, created_at, updated_at 
         FROM users WHERE id = ?1",
            rusqlite::params![id],
            |row| {
                Ok(User {
                    id: row.get(0)?,
                    username: row.get(1)?,
                    email: row.get(2)?,
                    password_hash: row.get(3)?,
                    created_at: row.get(4)?,
                    updated_at: row.get(5)?,
                })
            },
        )
        .map_err(|e| {
            error!("Database error fetching newly created user {}: {}", id, e);
            DbError::from(e.to_string())
        })?;

    info!(
        "User created successfully: id={}, username={}",
        id, username
    );
    Ok(user)
}

pub fn get_user_by_username(
    conn: &Arc<Mutex<rusqlite::Connection>>,
    username: &str,
) -> DbResult<Option<User>> {
    debug!("Fetching user by username: {}", username);

    let conn = conn.lock().map_err(|e| {
        error!("Failed to acquire database lock: {}", e);
        DbError::from("Failed to acquire database lock")
    })?;

    let user = conn
        .query_row(
            "SELECT id, username, email, password_hash, created_at, updated_at 
         FROM users WHERE username = ?1",
            rusqlite::params![username],
            |row| {
                Ok(User {
                    id: row.get(0)?,
                    username: row.get(1)?,
                    email: row.get(2)?,
                    password_hash: row.get(3)?,
                    created_at: row.get(4)?,
                    updated_at: row.get(5)?,
                })
            },
        )
        .optional()
        .map_err(|e| {
            error!(
                "Database error fetching user by username {}: {}",
                username, e
            );
            DbError::from(e.to_string())
        })?;

    if let Some(ref user) = user {
        debug!("User found: id={}, username={}", user.id, user.username);
    } else {
        debug!("No user found with username: {}", username);
    }

    Ok(user)
}

pub fn create_account(
    conn: &Arc<Mutex<rusqlite::Connection>>,
    user_id: &str,
    currency: &str,
    initial_balance: f64,
    country: Option<&str>,
) -> DbResult<Account> {
    debug!(
        "Creating new account: user_id={}, currency={}, balance={}, country={:?}",
        user_id, currency, initial_balance, country
    );

    let now = chrono::Utc::now().naive_utc();

    // Auto-set country based on currency if not provided
    let country_code = match country {
        Some(code) => code,
        None => {
            debug!("No country provided, deriving from currency: {}", currency);
            get_country_for_currency(currency).unwrap_or("IN") // Default to India if currency not found
        }
    };

    let conn = conn.lock().map_err(|e| {
        error!("Failed to acquire database lock: {}", e);
        DbError::from("Failed to acquire database lock")
    })?;

    conn.execute(
        "INSERT INTO accounts (id, currency, balance, country, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![user_id, currency, initial_balance, country_code, now, now],
    )
    .map_err(|e| {
        error!(
            "Database error creating account for user {}: {}",
            user_id, e
        );
        DbError::from(e.to_string())
    })?;

    let account = conn
        .query_row(
            "SELECT id, balance, currency, country, created_at, updated_at 
         FROM accounts WHERE id = ?1",
            rusqlite::params![user_id],
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
            error!(
                "Database error fetching newly created account for user {}: {}",
                user_id, e
            );
            DbError::from(e.to_string())
        })?;

    info!(
        "Account created successfully: user_id={}, currency={}, balance={}",
        user_id, currency, initial_balance
    );
    Ok(account)
}

pub fn get_account_by_id(
    conn: &Arc<Mutex<rusqlite::Connection>>,
    account_id: &str,
) -> DbResult<Option<Account>> {
    debug!("Fetching account by id: {}", account_id);

    let conn = conn.lock().map_err(|e| {
        error!("Failed to acquire database lock: {}", e);
        DbError::from("Failed to acquire database lock")
    })?;

    let account = conn
        .query_row(
            "SELECT id, balance, currency, country, created_at, updated_at 
         FROM accounts WHERE id = ?1",
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
        .optional()
        .map_err(|e| {
            error!("Database error fetching account {}: {}", account_id, e);
            DbError::from(e.to_string())
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

pub fn get_user_transactions(
    conn: &Arc<Mutex<rusqlite::Connection>>,
    account_id: &str,
) -> DbResult<Vec<Transaction>> {
    debug!("Fetching transactions for account: {}", account_id);

    let conn = conn.lock().map_err(|e| {
        error!("Failed to acquire database lock: {}", e);
        DbError::from("Failed to acquire database lock")
    })?;

    let mut stmt = conn
        .prepare(
            "SELECT id, from_account_id, to_account_id, amount, currency, 
                status, description, exchange_rate, original_amount, 
                original_currency, is_cross_border, created_at, updated_at
         FROM transactions
         WHERE from_account_id = ?1 OR to_account_id = ?1
         ORDER BY created_at DESC",
        )
        .map_err(|e| {
            error!(
                "Database error preparing transaction query for account {}: {}",
                account_id, e
            );
            DbError::from(e.to_string())
        })?;

    let transaction_iter = stmt
        .query_map(rusqlite::params![account_id], |row| {
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
        })
        .map_err(|e| {
            error!(
                "Database error fetching transactions for account {}: {}",
                account_id, e
            );
            DbError::from(e.to_string())
        })?;

    let mut transactions = Vec::new();
    for transaction in transaction_iter {
        transactions.push(transaction.map_err(|e| {
            error!("Error parsing transaction row: {}", e);
            DbError::from(e.to_string())
        })?);
    }

    info!(
        "Retrieved {} transactions for account {}",
        transactions.len(),
        account_id
    );
    Ok(transactions)
}
