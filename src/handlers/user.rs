use axum::{
    extract::State,
    routing::{get, post},
    Json, Router,
};
use tracing::{debug, error, info};
use validator::Validate;

use crate::{
    db::{create_account, get_account_by_id, DbError, DbPool},
    middleware::{auth::JwtAuth, AuthUser},
    models::{Account, CreateAccountRequest},
};

pub fn router() -> Router<(DbPool, JwtAuth)> {
    Router::new()
        .route("/accounts", post(create_user_account))
        .route("/me", get(get_current_user))
}

pub async fn create_user_account(
    State((pool, _)): State<(DbPool, JwtAuth)>,
    auth_user: AuthUser,
    Json(payload): Json<CreateAccountRequest>,
) -> Result<Json<Account>, String> {
    debug!(
        "Processing account creation request for user_id: {}",
        auth_user.user_id
    );

    // Validate request
    if let Err(errors) = payload.validate() {
        error!("Account creation validation failed: {:?}", errors);
        return Err(format!("Validation error: {:?}", errors));
    }

    // Create account with minimum balance and country info
    let account = create_account(
        &pool,
        &auth_user.user_id,
        &payload.currency,
        payload.minimum_balance,
        payload.country.as_deref(),
    )
    .await
    .map_err(|e: DbError| {
        error!(
            "Failed to create account for user {}: {}",
            auth_user.user_id, e
        );
        format!("Failed to create account: {}", e)
    })?;

    info!(
        "Account created successfully for user {}: {} {}",
        auth_user.user_id, account.balance, account.currency
    );
    Ok(Json(account))
}

pub async fn get_current_user(
    State((pool, _)): State<(DbPool, JwtAuth)>,
    auth_user: AuthUser,
) -> Result<Json<Account>, String> {
    debug!(
        "Processing get current user request for user_id: {}",
        auth_user.user_id
    );

    // Fetch the user's account details
    let account = get_account_by_id(&pool, &auth_user.user_id)
        .await
        .map_err(|e: DbError| {
            error!(
                "Database error fetching account for user {}: {}",
                auth_user.user_id, e
            );
            format!("Database error: {}", e)
        })?
        .ok_or_else(|| {
            error!("No account found for user: {}", auth_user.user_id);
            "Account not found for user".to_string()
        })?;

    info!(
        "Account details retrieved for user {}: {} {}",
        auth_user.user_id, account.balance, account.currency
    );
    Ok(Json(account))
}
