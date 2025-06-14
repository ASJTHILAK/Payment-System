use std::future::Future;
use std::time::Duration;
use thiserror::Error;
use tokio::time::sleep;
use tracing::{debug, warn};

#[derive(Debug, Error)]
pub enum RetryError<E> {
    #[error("All retry attempts exhausted")]
    Exhausted,
    #[error("Operation failed: {0}")]
    OperationFailed(E),
}

/// Configuration for retry mechanism
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// Maximum number of retry attempts (not including the initial attempt)
    pub max_retries: u32,
    /// Initial delay between retries
    pub initial_delay: Duration,
    /// Maximum delay between retries (to prevent excessive delays)
    pub max_delay: Duration,
    /// Multiplier for exponential backoff
    pub backoff_multiplier: f64,
    /// Optional jitter factor (0.0 to 1.0) to add randomness to delay
    pub jitter_factor: Option<f64>,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            initial_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(10),
            backoff_multiplier: 2.0,
            jitter_factor: Some(0.1), // 10% jitter
        }
    }
}

impl RetryConfig {
    /// Create a new retry configuration with custom settings
    pub fn new(max_retries: u32, initial_delay: Duration) -> Self {
        Self {
            max_retries,
            initial_delay,
            ..Default::default()
        }
    }

    /// Set the maximum delay between retries
    pub fn with_max_delay(mut self, max_delay: Duration) -> Self {
        self.max_delay = max_delay;
        self
    }

    /// Set the backoff multiplier for exponential backoff
    pub fn with_backoff_multiplier(mut self, multiplier: f64) -> Self {
        self.backoff_multiplier = multiplier;
        self
    }

    /// Set the jitter factor (0.0 to 1.0) to add randomness to delays
    pub fn with_jitter(mut self, jitter_factor: f64) -> Self {
        self.jitter_factor = Some(jitter_factor.clamp(0.0, 1.0));
        self
    }
}

/// Retry mechanism with exponential backoff and jitter
#[derive(Clone)]
pub struct RetryManager {
    config: RetryConfig,
}

impl RetryManager {
    /// Create a retry manager with the given configuration
    pub fn new(config: RetryConfig) -> Self {
        Self { config }
    }

    /// Execute an operation with retry logic
    /// The operation should return Ok(T) on success or Err(E) on failure
    /// Only retries if the should_retry predicate returns true for the error
    pub async fn execute<T, E, F, Fut, P>(
        &self,
        operation: F,
        should_retry: P,
    ) -> Result<T, RetryError<E>>
    where
        F: Fn() -> Fut,
        Fut: Future<Output = Result<T, E>>,
        P: Fn(&E) -> bool,
        E: std::fmt::Debug,
    {
        let mut attempt = 0;
        let mut delay = self.config.initial_delay;

        loop {
            debug!(
                "Executing operation, attempt {}/{}",
                attempt + 1,
                self.config.max_retries + 1
            );

            match operation().await {
                Ok(result) => {
                    if attempt > 0 {
                        debug!("Operation succeeded after {} retries", attempt);
                    }
                    return Ok(result);
                }
                Err(error) => {
                    // Check if we should retry this error
                    if !should_retry(&error) {
                        debug!("Error is not retryable: {:?}", error);
                        return Err(RetryError::OperationFailed(error));
                    }

                    // Check if we've exhausted our retries
                    if attempt >= self.config.max_retries {
                        warn!(
                            "All {} retry attempts exhausted, final error: {:?}",
                            self.config.max_retries, error
                        );
                        return Err(RetryError::Exhausted);
                    }

                    // Calculate delay with exponential backoff and optional jitter
                    let base_delay = delay.min(self.config.max_delay);
                    let actual_delay = if let Some(jitter_factor) = self.config.jitter_factor {
                        apply_jitter(base_delay, jitter_factor)
                    } else {
                        base_delay
                    };

                    warn!(
                        "Operation failed (attempt {}/{}), retrying in {:?}: {:?}",
                        attempt + 1,
                        self.config.max_retries + 1,
                        actual_delay,
                        error
                    );

                    // Wait before retrying
                    sleep(actual_delay).await;

                    // Increase delay for next attempt (exponential backoff)
                    delay = Duration::from_millis(
                        (delay.as_millis() as f64 * self.config.backoff_multiplier) as u64,
                    );

                    attempt += 1;
                }
            }
        }
    }
}

/// Apply jitter to a duration
fn apply_jitter(base_delay: Duration, jitter_factor: f64) -> Duration {
    use rand::Rng;

    let jitter_range = base_delay.as_millis() as f64 * jitter_factor;
    let mut rng = rand::thread_rng();
    let jitter = rng.gen_range(-jitter_range..=jitter_range);

    let final_delay_ms = (base_delay.as_millis() as f64 + jitter).max(0.0) as u64;
    Duration::from_millis(final_delay_ms)
}

/// Check if an error is retryable based on common patterns
pub fn is_retryable_error(error: &str) -> bool {
    let error_lower = error.to_lowercase();

    // Common retryable error patterns
    error_lower.contains("account balance changed during transaction")
        || error_lower.contains("please try again")
        || error_lower.contains("concurrent")
        || error_lower.contains("lock")
        || error_lower.contains("timeout")
        || error_lower.contains("temporary")
        || error_lower.contains("transient")
        || error_lower.contains("busy")
        || error_lower.contains("deadlock")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    #[tokio::test]
    async fn test_retry_success_on_first_attempt() {
        let retry_manager = RetryManager::new(RetryConfig::default());

        let result = retry_manager
            .execute(|| async { Ok::<i32, String>(42) }, |_| true)
            .await;

        assert_eq!(result.unwrap(), 42);
    }

    #[tokio::test]
    async fn test_retry_success_after_failures() {
        let retry_manager = RetryManager::new(RetryConfig::new(3, Duration::from_millis(10)));
        let attempt_count = Arc::new(AtomicU32::new(0));

        let attempt_count_clone = attempt_count.clone();
        let result = retry_manager
            .execute(
                move || {
                    let count = attempt_count_clone.fetch_add(1, Ordering::SeqCst);
                    async move {
                        if count < 2 {
                            Err("Temporary failure".to_string())
                        } else {
                            Ok(42)
                        }
                    }
                },
                |_| true,
            )
            .await;

        assert_eq!(result.unwrap(), 42);
        assert_eq!(attempt_count.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn test_retry_exhaustion() {
        let retry_manager = RetryManager::new(RetryConfig::new(2, Duration::from_millis(10)));

        let result = retry_manager
            .execute(
                || async { Err::<i32, String>("Persistent failure".to_string()) },
                |_| true,
            )
            .await;

        assert!(matches!(result, Err(RetryError::Exhausted)));
    }

    #[tokio::test]
    async fn test_non_retryable_error() {
        let retry_manager = RetryManager::new(RetryConfig::default());

        let result = retry_manager
            .execute(
                || async { Err::<i32, String>("Non-retryable error".to_string()) },
                |_| false, // Never retry
            )
            .await;

        assert!(matches!(result, Err(RetryError::OperationFailed(_))));
    }

    #[test]
    fn test_is_retryable_error() {
        assert!(is_retryable_error(
            "Account balance changed during transaction"
        ));
        assert!(is_retryable_error("Please try again"));
        assert!(is_retryable_error("Concurrent update detected"));
        assert!(is_retryable_error("Database lock error"));

        assert!(!is_retryable_error("Invalid account ID"));
        assert!(!is_retryable_error("Insufficient funds"));
        assert!(!is_retryable_error("Validation failed"));
    }
}
