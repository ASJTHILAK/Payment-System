use rusqlite::{Connection, Result};
use std::path::Path;
use std::sync::{Arc, Mutex};
use tracing::{debug, info};

/// Database connection handler
pub struct DbConnection {
    pub conn: Arc<Mutex<Connection>>,
}

impl DbConnection {
    /// Create a new database connection
    pub fn new(db_path: &str) -> Result<Self> {
        debug!("Attempting to create database connection at: {}", db_path);

        // Check if the database file exists
        let db_exists = Path::new(db_path).exists();
        debug!("Database file exists: {}", db_exists);

        // Ensure the directory exists
        if let Some(parent) = Path::new(db_path).parent() {
            if !parent.exists() {
                std::fs::create_dir_all(parent).map_err(|e| {
                    rusqlite::Error::SqliteFailure(
                        rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_CANTOPEN),
                        Some(format!("Failed to create directory: {}", e)),
                    )
                })?;
                debug!("Created directory: {:?}", parent);
            }
        }

        // Open the database connection
        let conn = Connection::open(db_path)?;
        info!("Database connection opened successfully at: {}", db_path);

        // If the database is new, create the schema
        if !db_exists {
            info!("Database file is new, creating schema...");
            crate::db::schema::create_schema(&conn)?;
            info!("Schema created successfully");
        } else {
            info!("Using existing database file");
        }

        Ok(DbConnection {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Get a connection to the database
    pub fn get(&self) -> Arc<Mutex<Connection>> {
        Arc::clone(&self.conn)
    }
}
