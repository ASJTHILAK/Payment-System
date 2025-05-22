use axum::{
    extract::State,
    routing::{get, post},
    Json, Router,
};
use tracing::{debug, error, info};
use validator::Validate;

use crate::{
    db::{get_user_transactions, process_transaction, DbError, DbPool},
    middleware::{auth::JwtAuth, AuthUser},
    models::{CreateTransactionRequest, Transaction},
};

pub fn router() -> Router<(DbPool, JwtAuth)> {
    Router::new()
        .route("/create", post(create))
        .route("/list", get(list))
}

pub async fn create(
    State((pool, _)): State<(DbPool, JwtAuth)>,
    auth_user: AuthUser,
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

    // Use the atomic transaction processing function
    let transaction = process_transaction(
        &pool,
        &auth_user.user_id,
        &payload.to_account_id,
        payload.amount,
        &payload.currency,
        payload.description.as_deref(),
    )
    .await
    .map_err(|e: DbError| {
        error!("Transaction processing failed: {}", e);
        format!("Transaction failed: {}", e)
    })?;

    info!(
        "Transaction completed: id={}, from={}, to={}, amount={}, currency={}",
        transaction.id, auth_user.user_id, payload.to_account_id, payload.amount, payload.currency
    );

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

    info!(
        "Retrieved {} transactions for user {}",
        transactions.len(),
        auth_user.user_id
    );

    Ok(Json(transactions))
}
