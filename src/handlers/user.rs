use axum::{
    extract::State,
    routing::{get, post},
    Json, Router,
};
use validator::Validate;

use crate::{
    db::{create_account, DbPool},
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
    // Validate request
    if let Err(errors) = payload.validate() {
        return Err(format!("Validation error: {:?}", errors));
    }

    // Create account with minimum balance
    let account = create_account(
        &pool,
        &auth_user.user_id,
        &payload.currency,
        payload.minimum_balance,
    )
    .await
    .map_err(|e| format!("Failed to create account: {}", e))?;

    Ok(Json(account))
}

pub async fn get_current_user(
    State((_pool, _)): State<(DbPool, JwtAuth)>,
    auth_user: AuthUser,
) -> Result<Json<String>, String> {
    Ok(Json(auth_user.user_id))
}
