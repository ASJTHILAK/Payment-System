pub use error::DbError;
pub use schema::*;
use sqlx::SqlitePool;

mod error;
mod schema;

pub type DbPool = SqlitePool;
