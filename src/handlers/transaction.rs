use axum::{
    extract::State,
    routing::{get, post},
    Json, Router,
};
use tracing::{debug, error, info};
use validator::Validate;

use crate::{
    db::{
        create_transaction, get_account_by_id, get_user_transactions, update_account_balance,
        update_transaction_status, DbError, DbPool,
    },
    middleware::{auth::JwtAuth, AuthUser},
    models::{CreateTransactionRequest, Transaction, TransactionStatus},
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

    // Get sender's account
    let from_account = get_account_by_id(&pool, &auth_user.user_id)
        .await
        .map_err(|e: DbError| {
            error!("Database error when fetching sender account: {}", e);
            format!("Database error: {}", e)
        })?
        .ok_or_else(|| {
            error!("Sender account not found: {}", auth_user.user_id);
            "Sender account not found".to_string()
        })?;

    // Get recipient's account
    let to_account = get_account_by_id(&pool, &payload.to_account_id)
        .await
        .map_err(|e: DbError| {
            error!("Database error when fetching recipient account: {}", e);
            format!("Database error: {}", e)
        })?
        .ok_or_else(|| {
            error!("Recipient account not found: {}", payload.to_account_id);
            "Recipient account not found".to_string()
        })?;

    // Check if currencies match
    if from_account.currency != payload.currency || to_account.currency != payload.currency {
        error!(
            "Currency mismatch: sender={}, recipient={}, requested={}",
            from_account.currency, to_account.currency, payload.currency
        );
        return Err("Currency mismatch".to_string());
    }

    // Check if sender has sufficient balance
    if from_account.balance < payload.amount {
        error!(
            "Insufficient balance: user={}, balance={}, amount={}",
            auth_user.user_id, from_account.balance, payload.amount
        );
        return Err("Insufficient balance".to_string());
    }

    // Create transaction
    let transaction = create_transaction(
        &pool,
        &from_account.id,
        &to_account.id,
        payload.amount,
        &payload.currency,
        payload.description.as_deref(),
    )
    .await
    .map_err(|e: DbError| {
        error!("Failed to create transaction record: {}", e);
        format!("Failed to create transaction: {}", e)
    })?;

    debug!("Created transaction with ID: {}", transaction.id);

    // Update account balances
    update_account_balance(
        &pool,
        &from_account.id,
        from_account.balance - payload.amount,
    )
    .await
    .map_err(|e: DbError| {
        error!("Failed to update sender balance: {}", e);
        format!("Failed to update sender balance: {}", e)
    })?;

    debug!(
        "Updated sender balance: user={}, new_balance={}",
        from_account.id,
        from_account.balance - payload.amount
    );

    update_account_balance(&pool, &to_account.id, to_account.balance + payload.amount)
        .await
        .map_err(|e: DbError| {
            error!("Failed to update recipient balance: {}", e);
            format!("Failed to update recipient balance: {}", e)
        })?;

    debug!(
        "Updated recipient balance: user={}, new_balance={}",
        to_account.id,
        to_account.balance + payload.amount
    );

    // Update transaction status
    update_transaction_status(&pool, &transaction.id, TransactionStatus::Completed)
        .await
        .map_err(|e: DbError| {
            error!("Failed to update transaction status: {}", e);
            format!("Failed to update transaction status: {}", e)
        })?;

    // Create a new transaction object with updated status
    let mut updated_transaction = transaction;
    updated_transaction.status = TransactionStatus::Completed;

    info!(
        "Transaction completed: id={}, from={}, to={}, amount={}, currency={}",
        updated_transaction.id, from_account.id, to_account.id, payload.amount, payload.currency
    );

    Ok(Json(updated_transaction))
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
