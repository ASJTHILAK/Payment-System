use axum::{extract::State, extract::TypedHeader, headers::Authorization, Json};
use payment_system::{
    db::connection::DbConnection,
    handlers::auth::{login, logout, refresh_token, register},
    middleware::auth::JwtAuth,
    models::{CreateUserRequest, LoginRequest, RefreshTokenRequest},
};
use std::sync::Arc;
use tempfile::TempDir;

// Mock DbConnection for testing
fn setup_test_db() -> Arc<DbConnection> {
    // Create a temporary directory that persists for the test
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test.db");
    let db_path_str = db_path.to_str().unwrap();

    let db_conn = DbConnection::new(db_path_str).expect("Failed to create test database");

    // Keep the temp directory alive by storing it in the connection
    std::mem::forget(temp_dir);

    Arc::new(db_conn)
}

#[tokio::test]
async fn test_register_success() {
    let db_conn = setup_test_db();
    let jwt_auth = JwtAuth::new(b"test_secret");

    let request = CreateUserRequest {
        username: "testuser".to_string(),
        email: "test@example.com".to_string(),
        password: "password123".to_string(),
    };

    let result = register(State((db_conn, jwt_auth)), Json(request)).await;

    assert!(result.is_ok(), "User registration should succeed");

    let user = result.unwrap().0;
    assert_eq!(user.username, "testuser");
    assert_eq!(user.email, "test@example.com");
}

#[tokio::test]
async fn test_register_invalid_email() {
    let db_conn = setup_test_db();
    let jwt_auth = JwtAuth::new(b"test_secret");

    let request = CreateUserRequest {
        username: "testuser".to_string(),
        email: "invalid-email".to_string(), // Invalid email format
        password: "password123".to_string(),
    };

    let result = register(State((db_conn, jwt_auth)), Json(request)).await;

    assert!(
        result.is_err(),
        "Registration with invalid email should fail"
    );
    assert!(result.unwrap_err().1.contains("Validation error"));
}

#[tokio::test]
async fn test_register_short_password() {
    let db_conn = setup_test_db();
    let jwt_auth = JwtAuth::new(b"test_secret");

    let request = CreateUserRequest {
        username: "testuser".to_string(),
        email: "test@example.com".to_string(),
        password: "short".to_string(), // Less than 8 characters
    };

    let result = register(State((db_conn, jwt_auth)), Json(request)).await;

    assert!(
        result.is_err(),
        "Registration with short password should fail"
    );
    assert!(result.unwrap_err().1.contains("Validation error"));
}

#[tokio::test]
async fn test_login_success() {
    let db_conn = setup_test_db();
    let jwt_auth = JwtAuth::new(b"test_secret");

    // First register a user
    let register_request = CreateUserRequest {
        username: "testuser".to_string(),
        email: "test@example.com".to_string(),
        password: "password123".to_string(),
    };

    let _ = register(
        State((db_conn.clone(), jwt_auth.clone())),
        Json(register_request),
    )
    .await;

    // Then try to login
    let login_request = LoginRequest {
        username: "testuser".to_string(),
        password: "password123".to_string(),
    };

    let result = login(State((db_conn, jwt_auth)), Json(login_request)).await;

    assert!(result.is_ok(), "Login should succeed");

    let response = result.unwrap().0;
    assert_eq!(response.token_type, "Bearer");
    assert!(!response.access_token.is_empty());
    assert!(!response.refresh_token.is_empty());
    assert_eq!(response.user.username, "testuser");
}

#[tokio::test]
async fn test_login_wrong_password() {
    let db_conn = setup_test_db();
    let jwt_auth = JwtAuth::new(b"test_secret");

    // First register a user
    let register_request = CreateUserRequest {
        username: "testuser".to_string(),
        email: "test@example.com".to_string(),
        password: "password123".to_string(),
    };

    let _ = register(
        State((db_conn.clone(), jwt_auth.clone())),
        Json(register_request),
    )
    .await;

    // Then try to login with wrong password
    let login_request = LoginRequest {
        username: "testuser".to_string(),
        password: "wrongpassword".to_string(),
    };

    let result = login(State((db_conn, jwt_auth)), Json(login_request)).await;

    assert!(result.is_err(), "Login with wrong password should fail");
    assert!(result.unwrap_err().1.contains("Invalid password"));
}

#[tokio::test]
async fn test_refresh_token() {
    let db_conn = setup_test_db();
    let jwt_auth = JwtAuth::new(b"test_secret");

    // First register and login a user
    let register_request = CreateUserRequest {
        username: "testuser".to_string(),
        email: "test@example.com".to_string(),
        password: "password123".to_string(),
    };

    let _ = register(
        State((db_conn.clone(), jwt_auth.clone())),
        Json(register_request),
    )
    .await;

    let login_request = LoginRequest {
        username: "testuser".to_string(),
        password: "password123".to_string(),
    };

    let login_result = login(
        State((db_conn.clone(), jwt_auth.clone())),
        Json(login_request),
    )
    .await
    .unwrap()
    .0;

    // Try to refresh the token
    let refresh_request = RefreshTokenRequest {
        refresh_token: login_result.refresh_token,
    };

    let result = refresh_token(State((db_conn, jwt_auth)), Json(refresh_request)).await;

    assert!(result.is_ok(), "Token refresh should succeed");
    let response = result.unwrap().0;
    assert_eq!(response.token_type, "Bearer");
    assert!(!response.access_token.is_empty());
}

#[tokio::test]
async fn test_logout() {
    let db_conn = setup_test_db();
    let jwt_auth = JwtAuth::new(b"test_secret");

    // First register and login a user
    let register_request = CreateUserRequest {
        username: "testuser".to_string(),
        email: "test@example.com".to_string(),
        password: "password123".to_string(),
    };

    let _ = register(
        State((db_conn.clone(), jwt_auth.clone())),
        Json(register_request),
    )
    .await;

    let login_request = LoginRequest {
        username: "testuser".to_string(),
        password: "password123".to_string(),
    };

    let login_result = login(
        State((db_conn.clone(), jwt_auth.clone())),
        Json(login_request),
    )
    .await
    .unwrap()
    .0;

    // Try to logout
    let auth_header =
        Authorization::bearer(&login_result.access_token).expect("Failed to create bearer token");

    let result = logout(State((db_conn, jwt_auth)), TypedHeader(auth_header)).await;

    assert!(result.is_ok(), "Logout should succeed");

    // Additional verification could be done by trying to use the token again
}
