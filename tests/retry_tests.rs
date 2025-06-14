// tests/retry_tests.rs
use payment_system::utils::retry::{is_retryable_error, RetryConfig, RetryManager};
use std::sync::{
    atomic::{AtomicU32, Ordering},
    Arc,
};
use std::time::Duration;

#[tokio::test]
async fn test_retry_mechanism_with_transaction_errors() {
    // Test that retryable transaction errors are properly retried
    let retry_config =
        RetryConfig::new(3, Duration::from_millis(10)).with_max_delay(Duration::from_millis(100));

    let retry_manager = RetryManager::new(retry_config);
    let attempt_count = Arc::new(AtomicU32::new(0));

    let attempt_count_clone = attempt_count.clone();
    let result = retry_manager
        .execute(
            move || {
                let count = attempt_count_clone.fetch_add(1, Ordering::SeqCst);
                async move {
                    if count < 2 {
                        // Simulate concurrency error that should be retried
                        Err(
                            "Account balance changed during transaction. Please try again."
                                .to_string(),
                        )
                    } else {
                        Ok("Transaction successful".to_string())
                    }
                }
            },
            |error: &String| is_retryable_error(error),
        )
        .await;

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "Transaction successful");
    assert_eq!(attempt_count.load(Ordering::SeqCst), 3);
}

#[tokio::test]
async fn test_non_retryable_transaction_errors() {
    // Test that non-retryable errors (like validation errors) are not retried
    let retry_config = RetryConfig::new(3, Duration::from_millis(10));
    let retry_manager = RetryManager::new(retry_config);
    let attempt_count = Arc::new(AtomicU32::new(0));

    let attempt_count_clone = attempt_count.clone();
    let result = retry_manager
        .execute(
            move || {
                attempt_count_clone.fetch_add(1, Ordering::SeqCst);
                async move {
                    // Simulate validation error that should NOT be retried
                    Err::<String, String>("Insufficient funds".to_string())
                }
            },
            |error: &String| is_retryable_error(error),
        )
        .await;

    assert!(result.is_err());
    // Should only attempt once since this is not a retryable error
    assert_eq!(attempt_count.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn test_retry_exhaustion_on_persistent_concurrency_errors() {
    // Test that persistent concurrency errors eventually exhaust retries
    let retry_config = RetryConfig::new(2, Duration::from_millis(5));

    let retry_manager = RetryManager::new(retry_config);
    let attempt_count = Arc::new(AtomicU32::new(0));

    let attempt_count_clone = attempt_count.clone();
    let result = retry_manager
        .execute(
            move || {
                attempt_count_clone.fetch_add(1, Ordering::SeqCst);
                async move {
                    // Always fail with retryable error
                    Err::<String, String>("Concurrent update detected".to_string())
                }
            },
            |error: &String| is_retryable_error(error),
        )
        .await;

    assert!(result.is_err());
    // Should attempt initial + 2 retries = 3 total attempts
    assert_eq!(attempt_count.load(Ordering::SeqCst), 3);
}

#[test]
fn test_retryable_error_patterns() {
    // Test that our error detection correctly identifies retryable errors
    assert!(is_retryable_error(
        "Account balance changed during transaction"
    ));
    assert!(is_retryable_error("Please try again"));
    assert!(is_retryable_error("Concurrent update detected"));
    assert!(is_retryable_error("Database lock error"));
    assert!(is_retryable_error("Connection timeout"));
    assert!(is_retryable_error("Temporary failure"));
    assert!(is_retryable_error("Transient error"));
    assert!(is_retryable_error("Database busy"));
    assert!(is_retryable_error("Deadlock detected"));

    // Test non-retryable errors
    assert!(!is_retryable_error("Invalid account ID"));
    assert!(!is_retryable_error("Insufficient funds"));
    assert!(!is_retryable_error("Validation failed"));
    assert!(!is_retryable_error("User not found"));
    assert!(!is_retryable_error("Invalid currency"));
    assert!(!is_retryable_error("Account does not exist"));
}
