use thiserror::Error;

/// Custom error type for database operations
#[derive(Error, Debug)]
pub enum DbError {
    /// Error from the rusqlite library
    #[error("Database error: {0}")]
    RusqliteError(String),

    /// Custom error with message
    #[error("{0}")]
    Custom(String),
}

/// Result type alias for database operations
pub type DbResult<T> = std::result::Result<T, DbError>;

impl From<&str> for DbError {
    fn from(message: &str) -> Self {
        DbError::Custom(message.to_string())
    }
}

impl From<String> for DbError {
    fn from(message: String) -> Self {
        DbError::Custom(message)
    }
}

impl From<rusqlite::Error> for DbError {
    fn from(error: rusqlite::Error) -> Self {
        DbError::RusqliteError(error.to_string())
    }
}
