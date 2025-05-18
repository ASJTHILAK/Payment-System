use sqlx::SqlitePool;

pub type DbPool = SqlitePool;

pub mod db;
pub mod handlers;
pub mod middleware;
pub mod models;
