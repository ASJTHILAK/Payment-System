// src/services/exchange_rate.rs
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::env;
use tracing::{debug, error, info};

use crate::db::{DbError, DbResult};

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
    pool: SqlitePool,
    api_key: String,
}

impl ExchangeRateService {
    pub fn new(pool: SqlitePool) -> Self {
        let api_key = env::var("EXCHANGE_RATE_API_KEY")
            .expect("EXCHANGE_RATE_API_KEY environment variable must be set");

        Self { pool, api_key }
    }

    /// Get the exchange rate from the database or fetch from API if needed
    pub async fn get_exchange_rate(
        &self,
        from_currency: &str,
        to_currency: &str,
    ) -> DbResult<ExchangeRate> {
        // First try to get from database
        if let Some(rate) = self.get_cached_rate(from_currency, to_currency).await? {
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
        self.fetch_and_cache_rate(from_currency, to_currency).await
    }

    /// Convert an amount from one currency to another
    pub async fn convert_currency(
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
        let exchange_rate = self.get_exchange_rate(from_currency, to_currency).await?;

        // Calculate the converted amount
        let converted_amount = amount * exchange_rate.rate;

        debug!(
            "Currency conversion: {} {} = {} {} (rate: {})",
            amount, from_currency, converted_amount, to_currency, exchange_rate.rate
        );

        Ok((converted_amount, exchange_rate.rate))
    }

    /// Get a cached rate from the database
    async fn get_cached_rate(
        &self,
        from_currency: &str,
        to_currency: &str,
    ) -> DbResult<Option<ExchangeRate>> {
        let rate = sqlx::query_as!(
            ExchangeRate,
            r#"
            SELECT 
                base_currency as "base_currency!",
                target_currency as "target_currency!",
                rate as "rate!",
                last_updated_at as "last_updated_at!"
            FROM exchange_rates
            WHERE base_currency = ? AND target_currency = ?
            "#,
            from_currency,
            to_currency
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| {
            error!("Database error fetching exchange rate: {}", e);
            DbError::from(e)
        })?;

        Ok(rate)
    }

    /// Fetch exchange rate from API and cache it in the database
    async fn fetch_and_cache_rate(
        &self,
        from_currency: &str,
        to_currency: &str,
    ) -> DbResult<ExchangeRate> {
        // Construct API URL
        let url = format!(
            "https://v6.exchangerate-api.com/v6/{}/latest/{}",
            self.api_key, from_currency
        );

        // Fetch from API
        let response = reqwest::get(&url).await.map_err(|e| {
            error!("Failed to fetch exchange rate from API: {}", e);
            DbError::from(format!("API request failed: {}", e))
        })?;

        // Check response status
        if !response.status().is_success() {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unable to read error response".to_string());
            error!("API request failed with status {}: {}", status, error_text);
            return Err(DbError::from(format!(
                "API request failed with status {}: {}",
                status, error_text
            )));
        }

        // Parse response
        let response = response.json::<ExchangeRateResponse>().await.map_err(|e| {
            error!("Failed to parse exchange rate API response: {}", e);
            DbError::from(format!("API response parsing failed: {}", e))
        })?;

        // Get the rate for the target currency
        let rate = response
            .conversion_rates
            .get(to_currency)
            .ok_or_else(|| {
                error!(
                    "Exchange rate not found for currency pair: {} to {}",
                    from_currency, to_currency
                );
                DbError::from(format!(
                    "Exchange rate not found for currency pair: {} to {}",
                    from_currency, to_currency
                ))
            })?
            .to_owned();

        // Save to database
        let now = chrono::Utc::now().naive_utc();

        sqlx::query!(
            r#"
            INSERT OR REPLACE INTO exchange_rates (
                base_currency, target_currency, rate, last_updated_at
            )
            VALUES (?, ?, ?, ?)
            "#,
            from_currency,
            to_currency,
            rate,
            now
        )
        .execute(&self.pool)
        .await
        .map_err(|e| {
            error!("Database error caching exchange rate: {}", e);
            DbError::from(e)
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
