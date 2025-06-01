use axum::{
    extract::{State, TypedHeader},
    headers::{authorization::Bearer, Authorization},
    routing::post,
    Json, Router,
};
use bcrypt::{hash, verify, DEFAULT_COST};
use std::sync::Arc;
use tracing::{debug, error, info};
use validator::Validate;

use crate::{
    db::{connection::DbConnection, create_user, get_user_by_username, DbError},
    middleware::auth::JwtAuth,
    models::{
        CreateUserRequest, LoginRequest, LoginResponse, RefreshTokenRequest, TokenResponse, User,
    },
};

pub fn router() -> Router<(Arc<DbConnection>, JwtAuth)> {
    Router::new()
        .route("/register", post(register))
        .route("/login", post(login))
        .route("/refresh", post(refresh_token))
        .route("/logout", post(logout))
}

pub async fn register(
    State((db_conn, _)): State<(Arc<DbConnection>, JwtAuth)>,
    Json(payload): Json<CreateUserRequest>,
) -> Result<Json<User>, (axum::http::StatusCode, String)> {
    use axum::http::StatusCode;

    debug!(
        "Processing register request for username: {}",
        payload.username
    );

    // Validate request
    if let Err(errors) = payload.validate() {
        error!("Registration validation failed: {:?}", errors);
        return Err((
            StatusCode::BAD_REQUEST,
            format!("Validation error: {:?}", errors),
        ));
    }

    // Check if user exists
    let existing_user =
        get_user_by_username(&db_conn.get(), &payload.username).map_err(|e: DbError| {
            error!("Database error checking existing username: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Database error: {}", e),
            )
        })?;

    if existing_user.is_some() {
        error!(
            "Registration failed: Username {} already exists",
            payload.username
        );
        return Err((StatusCode::CONFLICT, "Username already exists".to_string()));
    }

    // Hash password
    let password_hash = hash(payload.password.as_bytes(), DEFAULT_COST).map_err(|e| {
        error!("Failed to hash password: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to hash password: {}", e),
        )
    })?;

    // Create user
    let user = create_user(
        &db_conn.get(),
        &payload.username,
        &payload.email,
        &password_hash,
    )
    .map_err(|e: DbError| {
        error!("Failed to create user {}: {}", payload.username, e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to create user: {}", e),
        )
    })?;

    info!("User registered successfully: {}", user.username);
    Ok(Json(user))
}

pub async fn login(
    State((db_conn, jwt_auth)): State<(Arc<DbConnection>, JwtAuth)>,
    Json(payload): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, (axum::http::StatusCode, String)> {
    use axum::http::StatusCode;

    debug!(
        "Processing login request for username: {}",
        payload.username
    );

    // Get user
    let user = get_user_by_username(&db_conn.get(), &payload.username)
        .map_err(|e: DbError| {
            error!(
                "Database error during login attempt for {}: {}",
                payload.username, e
            );
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Database error: {}", e),
            )
        })?
        .ok_or_else(|| {
            error!("Login attempt for non-existent user: {}", payload.username);
            (StatusCode::NOT_FOUND, "User not found".to_string())
        })?;

    // Verify password
    if !verify(payload.password.as_bytes(), &user.password_hash).map_err(|e| {
        error!(
            "Failed to verify password for user {}: {}",
            payload.username, e
        );
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to verify password: {}", e),
        )
    })? {
        error!("Invalid password attempt for user: {}", payload.username);
        return Err((StatusCode::UNAUTHORIZED, "Invalid password".to_string()));
    }

    // Generate tokens
    let (access_token, refresh_token, expires_in) =
        jwt_auth.create_tokens(&user.id).await.map_err(|e| {
            error!(
                "Failed to create tokens for user {}: {}",
                payload.username, e
            );
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to create tokens: {}", e),
            )
        })?;

    info!("User {} logged in successfully", user.username);
    Ok(Json(LoginResponse {
        access_token,
        refresh_token,
        token_type: "Bearer".to_string(),
        expires_in,
        user,
    }))
}

pub async fn refresh_token(
    State((_, jwt_auth)): State<(Arc<DbConnection>, JwtAuth)>,
    Json(payload): Json<RefreshTokenRequest>,
) -> Result<Json<TokenResponse>, (axum::http::StatusCode, String)> {
    debug!("Processing token refresh request");

    let (access_token, expires_in) = jwt_auth
        .refresh_access_token(&payload.refresh_token)
        .await
        .map_err(|e| {
            error!("Token refresh failed: {}", e.1);
            e
        })?;

    info!("Token refreshed successfully");
    Ok(Json(TokenResponse {
        access_token,
        token_type: "Bearer".to_string(),
        expires_in,
    }))
}

pub async fn logout(
    State((_, jwt_auth)): State<(Arc<DbConnection>, JwtAuth)>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
) -> Result<(), (axum::http::StatusCode, String)> {
    debug!("Processing logout request");

    // Blacklist the current token
    jwt_auth
        .blacklist_token(bearer.token())
        .await
        .map_err(|e| {
            error!("Logout failed: {}", e.1);
            e
        })?;

    info!("User logged out successfully");
    Ok(())
}
