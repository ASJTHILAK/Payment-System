// src/services/exchange_rate.rs
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};
use std::{env, sync::Arc};
use tracing::{debug, error, info};

use crate::db::{connection::DbConnection, DbError, DbResult};

#[derive(Debug, Serialize, Deserialize)]
pub struct ExchangeRateResponse {
    pub base_code: String,
    pub conversion_rates: std::collections::HashMap<String, f64>,
}

/// Represents an exchange rate between two currencies
#[derive(Debug, Serialize, Deserialize)]
pub struct ExchangeRate {
    pub base_currency: String,
    pub target_currency: String,
    pub rate: f64,
    pub last_updated_at: chrono::NaiveDateTime,
}

/// Service for handling currency exchange rates
#[derive(Clone)]
pub struct ExchangeRateService {
    db_conn: Arc<DbConnection>,
    #[allow(dead_code)]
    api_key: String,
}

impl ExchangeRateService {
    pub fn new(db_conn: Arc<DbConnection>) -> Self {
        let api_key = env::var("EXCHANGE_RATE_API_KEY")
            .expect("EXCHANGE_RATE_API_KEY environment variable must be set");

        Self { db_conn, api_key }
    }

    /// Get the exchange rate from the database or fetch from API if needed
    pub fn get_exchange_rate(
        &self,
        from_currency: &str,
        to_currency: &str,
    ) -> DbResult<ExchangeRate> {
        // First try to get from database
        if let Some(rate) = self.get_cached_rate(from_currency, to_currency)? {
            // Check if rate is still fresh (less than 24 hours old)
            let now = chrono::Utc::now().naive_utc();

            if now.signed_duration_since(rate.last_updated_at) < chrono::Duration::hours(6) {
                debug!(
                    "Using cached exchange rate: {} to {} = {}",
                    from_currency, to_currency, rate.rate
                );
                return Ok(rate);
            }
        }

        // If not in database or outdated, fetch from API
        debug!(
            "Fetching fresh exchange rate: {} to {}",
            from_currency, to_currency
        );
        self.fetch_and_cache_rate(from_currency, to_currency)
    }

    /// Convert an amount from one currency to another
    pub fn convert_currency(
        &self,
        amount: f64,
        from_currency: &str,
        to_currency: &str,
    ) -> DbResult<(f64, f64)> {
        // Returns (converted_amount, exchange_rate)
        // If currencies are the same, no conversion needed
        if from_currency == to_currency {
            return Ok((amount, 1.0));
        }

        // Get the exchange rate
        let exchange_rate = self.get_exchange_rate(from_currency, to_currency)?;

        // Calculate the converted amount
        let converted_amount = amount * exchange_rate.rate;

        debug!(
            "Currency conversion: {} {} = {} {} (rate: {})",
            amount, from_currency, converted_amount, to_currency, exchange_rate.rate
        );

        Ok((converted_amount, exchange_rate.rate))
    }

    /// Get a cached rate from the database
    fn get_cached_rate(
        &self,
        from_currency: &str,
        to_currency: &str,
    ) -> DbResult<Option<ExchangeRate>> {
        let conn = self.db_conn.get();
        let conn = conn.lock().map_err(|e| {
            error!("Failed to acquire database lock: {}", e);
            DbError::from("Failed to acquire database lock")
        })?;

        let mut stmt = conn
            .prepare(
                "SELECT base_currency, target_currency, rate, last_updated_at 
             FROM exchange_rates 
             WHERE base_currency = ?1 AND target_currency = ?2",
            )
            .map_err(|e| {
                error!("Failed to prepare statement: {}", e);
                DbError::from(format!("Database error: {}", e))
            })?;

        let rate = stmt
            .query_row(rusqlite::params![from_currency, to_currency], |row| {
                Ok(ExchangeRate {
                    base_currency: row.get(0)?,
                    target_currency: row.get(1)?,
                    rate: row.get(2)?,
                    last_updated_at: row.get(3)?,
                })
            })
            .optional()
            .map_err(|e| {
                error!("Database error fetching exchange rate: {}", e);
                DbError::from(e.to_string())
            })?;

        Ok(rate)
    }

    /// Fetch exchange rate from API and cache it in the database
    fn fetch_and_cache_rate(
        &self,
        from_currency: &str,
        to_currency: &str,
    ) -> DbResult<ExchangeRate> {
        // For this version, we'll use a simple mock implementation
        // In a real-world scenario, you'd make the HTTP call using a blocking client

        // Mock exchange rates for demonstration
        let rate = match (from_currency, to_currency) {
            ("USD", "EUR") => 0.85,
            ("EUR", "USD") => 1.18,
            ("USD", "GBP") => 0.73,
            ("GBP", "USD") => 1.37,
            ("USD", "INR") => 74.50,
            ("INR", "USD") => 0.0134,
            ("EUR", "INR") => 87.83,
            ("INR", "EUR") => 0.0114,
            ("GBP", "INR") => 102.05,
            ("INR", "GBP") => 0.0098,
            _ => {
                error!(
                    "Exchange rate not found for currency pair: {} to {}",
                    from_currency, to_currency
                );
                return Err(DbError::from(format!(
                    "Exchange rate not found for currency pair: {} to {}",
                    from_currency, to_currency
                )));
            }
        };

        // Save to database
        let now = chrono::Utc::now().naive_utc();

        let conn = self.db_conn.get();
        let conn = conn.lock().map_err(|e| {
            error!("Failed to acquire database lock: {}", e);
            DbError::from("Failed to acquire database lock")
        })?;

        conn.execute(
            "INSERT OR REPLACE INTO exchange_rates (
                base_currency, target_currency, rate, last_updated_at
            ) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![from_currency, to_currency, rate, now],
        )
        .map_err(|e| {
            error!("Database error caching exchange rate: {}", e);
            DbError::from(e.to_string())
        })?;

        info!(
            "Cached exchange rate: {} to {} = {}",
            from_currency, to_currency, rate
        );

        Ok(ExchangeRate {
            base_currency: from_currency.to_string(),
            target_currency: to_currency.to_string(),
            rate,
            last_updated_at: now,
        })
    }
}
