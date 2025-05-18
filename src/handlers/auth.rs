use axum::{
    extract::{State, TypedHeader},
    headers::{authorization::Bearer, Authorization},
    routing::post,
    Json, Router,
};
use bcrypt::{hash, verify, DEFAULT_COST};
use validator::Validate;

use crate::{
    db::{create_user, get_user_by_username, DbPool},
    middleware::auth::JwtAuth,
    models::{
        CreateUserRequest, LoginRequest, LoginResponse, RefreshTokenRequest, TokenResponse, User,
    },
};

pub fn router() -> Router<(DbPool, JwtAuth)> {
    Router::new()
        .route("/register", post(register))
        .route("/login", post(login))
        .route("/refresh", post(refresh_token))
        .route("/logout", post(logout))
}

pub async fn register(
    State((pool, _)): State<(DbPool, JwtAuth)>,
    Json(payload): Json<CreateUserRequest>,
) -> Result<Json<User>, (axum::http::StatusCode, String)> {
    use axum::http::StatusCode;

    // Validate request
    if let Err(errors) = payload.validate() {
        return Err((
            StatusCode::BAD_REQUEST,
            format!("Validation error: {:?}", errors),
        ));
    }

    // Check if user exists
    if let Ok(Some(_)) = get_user_by_username(&pool, &payload.username).await {
        return Err((StatusCode::CONFLICT, "Username already exists".to_string()));
    }

    // Hash password
    let password_hash = hash(payload.password.as_bytes(), DEFAULT_COST).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to hash password: {}", e),
        )
    })?;

    // Create user
    let user = create_user(&pool, &payload.username, &payload.email, &password_hash)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to create user: {}", e),
            )
        })?;

    Ok(Json(user))
}

pub async fn login(
    State((pool, jwt_auth)): State<(DbPool, JwtAuth)>,
    Json(payload): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, (axum::http::StatusCode, String)> {
    use axum::http::StatusCode;

    // Get user
    let user = get_user_by_username(&pool, &payload.username)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Database error: {}", e),
            )
        })?
        .ok_or_else(|| (StatusCode::NOT_FOUND, "User not found".to_string()))?;

    // Verify password
    if !verify(payload.password.as_bytes(), &user.password_hash).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to verify password: {}", e),
        )
    })? {
        return Err((StatusCode::UNAUTHORIZED, "Invalid password".to_string()));
    }

    // Generate tokens
    let (access_token, refresh_token, expires_in) =
        jwt_auth.create_tokens(&user.id).await.map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to create tokens: {}", e),
            )
        })?;

    Ok(Json(LoginResponse {
        access_token,
        refresh_token,
        token_type: "Bearer".to_string(),
        expires_in,
        user,
    }))
}

pub async fn refresh_token(
    State((_, jwt_auth)): State<(DbPool, JwtAuth)>,
    Json(payload): Json<RefreshTokenRequest>,
) -> Result<Json<TokenResponse>, (axum::http::StatusCode, String)> {
    let (access_token, expires_in) = jwt_auth
        .refresh_access_token(&payload.refresh_token)
        .await?;

    Ok(Json(TokenResponse {
        access_token,
        token_type: "Bearer".to_string(),
        expires_in,
    }))
}

pub async fn logout(
    State((_, jwt_auth)): State<(DbPool, JwtAuth)>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
) -> Result<(), (axum::http::StatusCode, String)> {
    // Blacklist the current token
    jwt_auth.blacklist_token(bearer.token()).await?;
    Ok(())
}
