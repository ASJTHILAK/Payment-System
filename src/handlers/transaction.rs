use axum::{
    extract::State,
    routing::{get, post},
    Json, Router,
};
use validator::Validate;

use crate::{
    db::{
        create_transaction, get_account_by_id, get_user_transactions, update_account_balance,
        update_transaction_status, DbPool,
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
    // Validate request
    if let Err(errors) = payload.validate() {
        return Err(format!("Validation error: {:?}", errors));
    }

    // Get sender's account
    let from_account = get_account_by_id(&pool, &auth_user.user_id)
        .await
        .map_err(|e| format!("Database error: {}", e))?
        .ok_or_else(|| "Sender account not found".to_string())?;

    // Get recipient's account
    let to_account = get_account_by_id(&pool, &payload.to_account_id)
        .await
        .map_err(|e| format!("Database error: {}", e))?
        .ok_or_else(|| "Recipient account not found".to_string())?;

    // Check if currencies match
    if from_account.currency != payload.currency || to_account.currency != payload.currency {
        return Err("Currency mismatch".to_string());
    }

    // Check if sender has sufficient balance
    if from_account.balance < payload.amount {
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
    .map_err(|e| format!("Failed to create transaction: {}", e))?;

    // Update account balances
    update_account_balance(
        &pool,
        &from_account.id,
        from_account.balance - payload.amount,
    )
    .await
    .map_err(|e| format!("Failed to update sender balance: {}", e))?;

    update_account_balance(&pool, &to_account.id, to_account.balance + payload.amount)
        .await
        .map_err(|e| format!("Failed to update recipient balance: {}", e))?;

    // Update transaction status
    update_transaction_status(&pool, &transaction.id, TransactionStatus::Completed)
        .await
        .map_err(|e| format!("Failed to update transaction status: {}", e))?;

    Ok(Json(transaction))
}

pub async fn list(
    State((pool, _)): State<(DbPool, JwtAuth)>,
    auth_user: AuthUser,
) -> Result<Json<Vec<Transaction>>, String> {
    let transactions = get_user_transactions(&pool, &auth_user.user_id)
        .await
        .map_err(|e| format!("Failed to get transactions: {}", e))?;

    Ok(Json(transactions))
}
