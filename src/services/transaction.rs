use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, info};

use crate::{
    db::{connection::DbConnection, process_cross_border_transaction, process_transaction},
    models::Transaction,
    services::{compliance::ComplianceService, exchange_rate::ExchangeRateService},
    utils::retry::{is_retryable_error, RetryConfig, RetryError, RetryManager},
};

/// Service for handling transaction operations with retry capabilities
#[derive(Clone)]
pub struct TransactionService {
    db_conn: Arc<DbConnection>,
    retry_manager: RetryManager,
}

impl TransactionService {
    /// Create a new transaction service with default retry configuration
    pub fn new(db_conn: Arc<DbConnection>) -> Self {
        let retry_config = RetryConfig::new(3, Duration::from_millis(100))
            .with_max_delay(Duration::from_secs(5))
            .with_backoff_multiplier(2.0)
            .with_jitter(0.1);

        Self {
            db_conn,
            retry_manager: RetryManager::new(retry_config),
        }
    }

    /// Create a new transaction service with custom retry configuration
    #[allow(dead_code)]
    pub fn with_retry_config(db_conn: Arc<DbConnection>, retry_config: RetryConfig) -> Self {
        Self {
            db_conn,
            retry_manager: RetryManager::new(retry_config),
        }
    }

    /// Process a standard transaction with automatic retry on concurrency conflicts
    pub async fn process_transaction_with_retry(
        &self,
        from_account_id: String,
        to_account_id: String,
        amount: f64,
        currency: String,
        description: Option<String>,
    ) -> Result<Transaction, TransactionError> {
        let db_conn = self.db_conn.clone();

        debug!(
            "Starting transaction with retry: from={}, to={}, amount={}, currency={}",
            from_account_id, to_account_id, amount, currency
        );

        // Store the original values for logging
        let orig_from_account_id = from_account_id.clone();
        let orig_to_account_id = to_account_id.clone();
        let orig_currency = currency.clone();

        let result = self
            .retry_manager
            .execute(
                move || {
                    let db_conn = db_conn.clone();
                    let from_account_id = from_account_id.clone();
                    let to_account_id = to_account_id.clone();
                    let currency = currency.clone();
                    let description = description.clone();

                    async move {
                        process_transaction(
                            &db_conn.get(),
                            &from_account_id,
                            &to_account_id,
                            amount,
                            &currency,
                            description.as_deref(),
                        )
                        .map_err(|e| e.to_string())
                    }
                },
                |error: &String| is_retryable_error(error),
            )
            .await;

        match result {
            Ok(transaction) => {
                info!(
                    "Transaction completed successfully: id={}, from={}, to={}, amount={}, currency={}",
                    transaction.id, orig_from_account_id, orig_to_account_id, amount, orig_currency
                );
                Ok(transaction)
            }
            Err(RetryError::Exhausted) => {
                Err(TransactionError::RetriesExhausted(format!(
                    "Transaction failed after all retry attempts: from={}, to={}, amount={}, currency={}",
                    orig_from_account_id, orig_to_account_id, amount, orig_currency
                )))
            }
            Err(RetryError::OperationFailed(error)) => {
                Err(TransactionError::OperationFailed(error))
            }
        }
    }

    /// Process a cross-border transaction with automatic retry on concurrency conflicts
    pub async fn process_cross_border_transaction_with_retry(
        &self,
        exchange_rate_service: ExchangeRateService,
        compliance_service: ComplianceService,
        from_account_id: String,
        to_account_id: String,
        amount: f64,
        currency: String,
        auto_convert: bool,
        description: Option<String>,
    ) -> Result<Transaction, TransactionError> {
        let db_conn = self.db_conn.clone();

        debug!(
            "Starting cross-border transaction with retry: from={}, to={}, amount={}, currency={}, auto_convert={}",
            from_account_id, to_account_id, amount, currency, auto_convert
        );

        // Store the original values for logging
        let orig_from_account_id = from_account_id.clone();
        let orig_to_account_id = to_account_id.clone();
        let orig_currency = currency.clone();

        let result = self
            .retry_manager
            .execute(
                move || {
                    let db_conn = db_conn.clone();
                    let exchange_rate_service = exchange_rate_service.clone();
                    let compliance_service = compliance_service.clone();
                    let from_account_id = from_account_id.clone();
                    let to_account_id = to_account_id.clone();
                    let currency = currency.clone();
                    let description = description.clone();

                    async move {
                        process_cross_border_transaction(
                            &db_conn.get(),
                            &exchange_rate_service,
                            &compliance_service,
                            &from_account_id,
                            &to_account_id,
                            amount,
                            &currency,
                            auto_convert,
                            description.as_deref(),
                        )
                        .map_err(|e| e.to_string())
                    }
                },
                |error: &String| is_retryable_error(error),
            )
            .await;

        match result {
            Ok(transaction) => {
                info!(
                    "Cross-border transaction completed successfully: id={}, from={}, to={}, amount={}, currency={}, auto_convert={}",
                    transaction.id, orig_from_account_id, orig_to_account_id, amount, orig_currency, auto_convert
                );
                Ok(transaction)
            }
            Err(RetryError::Exhausted) => {
                Err(TransactionError::RetriesExhausted(format!(
                    "Cross-border transaction failed after all retry attempts: from={}, to={}, amount={}, currency={}",
                    orig_from_account_id, orig_to_account_id, amount, orig_currency
                )))
            }
            Err(RetryError::OperationFailed(error)) => {
                Err(TransactionError::OperationFailed(error))
            }
        }
    }
}

/// Error types for transaction operations
#[derive(Debug, thiserror::Error)]
pub enum TransactionError {
    #[error("Transaction failed after all retry attempts: {0}")]
    RetriesExhausted(String),
    #[error("Transaction operation failed: {0}")]
    OperationFailed(String),
}

impl From<TransactionError> for String {
    fn from(error: TransactionError) -> Self {
        error.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::connection::DbConnection;
    use tempfile::TempDir;

    fn setup_test_db() -> Arc<DbConnection> {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let db_path = temp_dir.path().join("test.db");
        let db_path_str = db_path.to_str().unwrap();

        let db_conn = DbConnection::new(db_path_str).expect("Failed to create test database");
        let db_conn = Arc::new(db_conn);

        // Create test users and accounts
        let sender_id = "00000000-0000-0000-0000-000000000000";
        let recipient_id = "00000000-0000-0000-0000-000000000001";

        {
            let conn = db_conn.conn.lock().unwrap();

            conn.execute(
                "INSERT INTO users (id, username, email, password_hash, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)",
                [sender_id, "sender", "sender@example.com", "password_hash"],
            )
            .expect("Failed to create sender user");

            conn.execute(
                "INSERT INTO users (id, username, email, password_hash, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)",
                [
                    recipient_id,
                    "recipient",
                    "recipient@example.com",
                    "password_hash",
                ],
            )
            .expect("Failed to create recipient user");

            conn.execute(
                "INSERT INTO accounts (id, currency, balance, created_at, updated_at)
                 VALUES (?1, ?2, ?3, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)",
                [sender_id, "INR", "5000.0"],
            )
            .expect("Failed to create sender account");

            conn.execute(
                "INSERT INTO accounts (id, currency, balance, created_at, updated_at)
                 VALUES (?1, ?2, ?3, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)",
                [recipient_id, "INR", "1000.0"],
            )
            .expect("Failed to create recipient account");
        }

        std::mem::forget(temp_dir);
        db_conn
    }

    #[tokio::test]
    async fn test_transaction_service_success() {
        let db_conn = setup_test_db();
        let transaction_service = TransactionService::new(db_conn.clone());

        let result = transaction_service
            .process_transaction_with_retry(
                "00000000-0000-0000-0000-000000000000".to_string(),
                "00000000-0000-0000-0000-000000000001".to_string(),
                1000.0,
                "INR".to_string(),
                Some("Test transaction".to_string()),
            )
            .await;

        assert!(result.is_ok());
        let transaction = result.unwrap();
        assert_eq!(transaction.amount, 1000.0);
        assert_eq!(transaction.currency, "INR");
    }

    #[tokio::test]
    async fn test_transaction_service_with_custom_retry_config() {
        let db_conn = setup_test_db();

        // Custom retry config with fewer retries and shorter delays for testing
        let retry_config = RetryConfig::new(1, Duration::from_millis(10))
            .with_max_delay(Duration::from_millis(50));

        let transaction_service =
            TransactionService::with_retry_config(db_conn.clone(), retry_config);

        let result = transaction_service
            .process_transaction_with_retry(
                "00000000-0000-0000-0000-000000000000".to_string(),
                "00000000-0000-0000-0000-000000000001".to_string(),
                500.0,
                "INR".to_string(),
                Some("Test transaction with custom config".to_string()),
            )
            .await;

        assert!(result.is_ok());
        let transaction = result.unwrap();
        assert_eq!(transaction.amount, 500.0);
    }
}
