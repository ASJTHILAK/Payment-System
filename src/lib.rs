use sqlx::SqlitePool;

pub type DbPool = SqlitePool;

pub mod db;
pub mod error;
pub mod handlers;
pub mod middleware;
pub mod models;
pub mod services;

pub use error::AppError;
