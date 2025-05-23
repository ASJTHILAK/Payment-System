use axum::extract::Path;
use axum::{
    extract::State,
    routing::{get, post},
    Extension, Json, Router,
};
use serde::Serialize;
use tracing::{debug, error, info};
use validator::Validate;

use crate::{
    db::{
        get_account_by_id, get_user_transactions, process_cross_border_transaction,
        process_transaction, DbError, DbPool,
    },
    middleware::{auth::JwtAuth, AuthUser},
    models::{CreateTransactionRequest, Transaction},
    services::{compliance::ComplianceService, exchange_rate::ExchangeRateService},
};

pub fn router() -> Router<(DbPool, JwtAuth)> {
    Router::new()
        .route("/create", post(create))
        .route("/list", get(list))
        .route(
            "/compliance/:transaction_id",
            get(get_transaction_compliance),
        )
        .route("/exchange-rates/:currency", get(get_exchange_rates))
}

#[derive(Debug, Serialize)]
pub struct ComplianceResponse {
    pub transaction_id: String,
    pub is_cross_border: bool,
    pub source_country: String,
    pub destination_country: String,
    pub risk_score: f64,
    pub status: String,
    pub details: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ExchangeRateResponse {
    pub base_currency: String,
    pub rates: std::collections::HashMap<String, f64>,
    pub last_updated_at: String,
}

pub async fn create(
    State((pool, _)): State<(DbPool, JwtAuth)>,
    auth_user: AuthUser,
    Extension(exchange_rate_service): Extension<ExchangeRateService>,
    Extension(compliance_service): Extension<ComplianceService>,
    Json(payload): Json<CreateTransactionRequest>,
) -> Result<Json<Transaction>, String> {
    debug!(
        "Processing transaction creation: from={} to={} amount={} currency={}",
        auth_user.user_id, payload.to_account_id, payload.amount, payload.currency
    );

    // Validate request
    if let Err(errors) = payload.validate() {
        error!("Transaction validation failed: {:?}", errors);
        return Err(format!("Validation error: {:?}", errors));
    }

    // Check if this might be a cross-border payment that needs conversion
    let convert_currency = payload.convert_currency.unwrap_or(false);

    let transaction = if convert_currency {
        // Use the cross-border transaction processing function with potential currency conversion
        debug!("Processing as cross-border payment with potential currency conversion");
        process_cross_border_transaction(
            &pool,
            &exchange_rate_service,
            &compliance_service,
            &auth_user.user_id,
            &payload.to_account_id,
            payload.amount,
            &payload.currency,
            true,
            payload.description.as_deref(),
        )
        .await
    } else {
        // Use the standard transaction processing function
        debug!("Processing as standard domestic payment");
        process_transaction(
            &pool,
            &auth_user.user_id,
            &payload.to_account_id,
            payload.amount,
            &payload.currency,
            payload.description.as_deref(),
        )
        .await
    }
    .map_err(|e: DbError| {
        error!("Transaction processing failed: {}", e);
        format!("Transaction failed: {}", e)
    })?;

    if convert_currency && transaction.is_cross_border.unwrap_or(false) {
        info!(
            "Cross-border transaction completed: id={}, from={}, to={}, amount={} {}, original_amount={} {}",
            transaction.id,
            auth_user.user_id,
            payload.to_account_id,
            transaction.amount,
            transaction.currency,
            transaction.original_amount.unwrap_or(transaction.amount),
            transaction.original_currency.as_deref().unwrap_or(&transaction.currency)
        );
    } else {
        info!(
            "Transaction completed: id={}, from={}, to={}, amount={}, currency={}",
            transaction.id,
            auth_user.user_id,
            payload.to_account_id,
            payload.amount,
            payload.currency
        );
    }

    Ok(Json(transaction))
}

pub async fn list(
    State((pool, _)): State<(DbPool, JwtAuth)>,
    auth_user: AuthUser,
) -> Result<Json<Vec<Transaction>>, String> {
    debug!("Listing transactions for user: {}", auth_user.user_id);

    let transactions = get_user_transactions(&pool, &auth_user.user_id)
        .await
        .map_err(|e: DbError| {
            error!("Failed to fetch transactions: {}", e);
            format!("Failed to get transactions: {}", e)
        })?;

    // Add custom output for cross-border transactions
    for transaction in &transactions {
        if transaction.is_cross_border.unwrap_or(false) {
            debug!(
                "Cross-border transaction: id={}, exchange_rate={}, original_amount={} {}",
                transaction.id,
                transaction.exchange_rate.unwrap_or(1.0),
                transaction.original_amount.unwrap_or(transaction.amount),
                transaction
                    .original_currency
                    .as_deref()
                    .unwrap_or(&transaction.currency)
            );
        }
    }

    info!(
        "Retrieved {} transactions for user {}",
        transactions.len(),
        auth_user.user_id
    );

    Ok(Json(transactions))
}

pub async fn get_transaction_compliance(
    State((pool, _)): State<(DbPool, JwtAuth)>,
    auth_user: AuthUser,
    Extension(compliance_service): Extension<ComplianceService>,
    Path(transaction_id): Path<String>,
) -> Result<Json<ComplianceResponse>, String> {
    debug!(
        "Fetching compliance information for transaction: {}",
        transaction_id
    );

    // First verify that the transaction belongs to the authenticated user
    let transactions = get_user_transactions(&pool, &auth_user.user_id)
        .await
        .map_err(|e: DbError| {
            error!("Failed to fetch transactions: {}", e);
            format!("Failed to get transactions: {}", e)
        })?;

    // Find the requested transaction
    let transaction = transactions
        .iter()
        .find(|t| t.id == transaction_id)
        .ok_or_else(|| "Transaction not found or does not belong to this user".to_string())?;

    // Check if this is a cross-border transaction
    if !transaction.is_cross_border.unwrap_or(false) {
        return Err("This transaction does not have compliance check information".to_string());
    }

    // Fetch compliance information
    let compliance_check = compliance_service
        .get_compliance_check_by_transaction(&transaction_id)
        .await
        .map_err(|e: DbError| {
            error!("Failed to fetch compliance information: {}", e);
            format!("Compliance check failed: {}", e)
        })?
        .ok_or_else(|| "No compliance information found for this transaction".to_string())?;

    // Get from_account country and to_account country
    let from_account = get_account_by_id(&pool, &transaction.from_account_id)
        .await
        .map_err(|e: DbError| {
            error!("Failed to fetch sender account info: {}", e);
            format!("Account lookup failed: {}", e)
        })?
        .ok_or_else(|| "Sender account not found".to_string())?;

    let to_account = get_account_by_id(&pool, &transaction.to_account_id)
        .await
        .map_err(|e: DbError| {
            error!("Failed to fetch receiver account info: {}", e);
            format!("Account lookup failed: {}", e)
        })?
        .ok_or_else(|| "Receiver account not found".to_string())?;

    let source_country = from_account
        .country
        .unwrap_or_else(|| "UNKNOWN".to_string());
    let destination_country = to_account.country.unwrap_or_else(|| "UNKNOWN".to_string());

    let response = ComplianceResponse {
        transaction_id: transaction.id.clone(),
        is_cross_border: true,
        source_country,
        destination_country,
        risk_score: compliance_check.risk_score,
        status: compliance_check.status.to_string(),
        details: compliance_check.details,
    };

    info!(
        "Compliance information retrieved for transaction: {}, status: {}",
        transaction_id, response.status
    );

    Ok(Json(response))
}

pub async fn get_exchange_rates(
    State((_, _)): State<(DbPool, JwtAuth)>,
    auth_user: AuthUser,
    Extension(exchange_rate_service): Extension<ExchangeRateService>,
    Path(currency): Path<String>,
) -> Result<Json<ExchangeRateResponse>, String> {
    debug!(
        "Fetching exchange rates for currency: {} by user: {}",
        currency, auth_user.user_id
    );

    // List of common currencies to show rates for
    let target_currencies = vec![
        "USD", "EUR", "GBP", "JPY", "AUD", "CAD", "CHF", "INR", "SGD", "AED",
    ];

    let mut rates = std::collections::HashMap::new();
    // Use DateTime instead of NaiveDateTime for timestamp handling
    let mut last_updated = chrono::DateTime::from_timestamp(0, 0)
        .map(|dt| dt.naive_utc())
        .unwrap_or_else(|| chrono::NaiveDateTime::MIN);

    // Get exchange rates for each target currency
    for target_currency in target_currencies {
        // Skip if it's the same as the base currency
        if target_currency == currency {
            rates.insert(target_currency.to_string(), 1.0);
            continue;
        }

        // Get the exchange rate
        match exchange_rate_service
            .get_exchange_rate(&currency, target_currency)
            .await
        {
            Ok(rate) => {
                rates.insert(target_currency.to_string(), rate.rate);
                if rate.last_updated_at > last_updated {
                    last_updated = rate.last_updated_at;
                }
            }
            Err(e) => {
                error!(
                    "Failed to fetch exchange rate from {} to {}: {}",
                    currency, target_currency, e
                );
                // Skip this rate, but continue with others
            }
        }
    }

    let response = ExchangeRateResponse {
        base_currency: currency.clone(),
        rates,
        last_updated_at: last_updated.format("%Y-%m-%d %H:%M:%S").to_string(),
    };

    info!(
        "Exchange rates retrieved for {} with {} target currencies",
        currency,
        response.rates.len()
    );

    Ok(Json(response))
}
