pub use schema::*;
use sqlx::SqlitePool;

mod schema;

pub type DbPool = SqlitePool;
// pub type DbError = sqlx::Error;
