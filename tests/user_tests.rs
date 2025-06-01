use axum::{extract::State, Json};
use payment_system::{
    db::connection::DbConnection,
    handlers::user::{create_user_account, get_current_user},
    middleware::{auth::JwtAuth, AuthUser},
    models::CreateAccountRequest,
};
use std::sync::Arc;
use tempfile::TempDir;

// Mock AuthUser for testing
fn create_test_auth_user() -> AuthUser {
    AuthUser {
        user_id: "test_user_id".to_string(),
    }
}

// Mock DbConnection for testing
fn setup_test_db() -> Arc<DbConnection> {
    // Create a temporary directory that persists for the test
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test.db");
    let db_path_str = db_path.to_str().unwrap();

    let db_conn = DbConnection::new(db_path_str).expect("Failed to create test database");

    let db_conn = Arc::new(db_conn);

    // Create a test user since accounts are linked to users
    let user_id = "test_user_id";
    {
        let conn = db_conn.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO users (id, username, email, password_hash, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)",
            [user_id, "testuser", "test@example.com", "password_hash"],
        )
        .expect("Failed to create test user");
    }

    // Keep the temp directory alive by storing it in the connection
    std::mem::forget(temp_dir);

    db_conn
}

#[tokio::test]
async fn test_create_user_account_success() {
    let db_conn = setup_test_db();
    let jwt_auth = JwtAuth::new(b"test_secret");
    let auth_user = create_test_auth_user();

    let request = CreateAccountRequest {
        currency: "INR".to_string(),
        minimum_balance: 1000.0,
        country: Some("IN".to_string()),
    };

    let result = create_user_account(State((db_conn, jwt_auth)), auth_user, Json(request)).await;

    assert!(result.is_ok(), "Account creation should succeed");

    let account = result.unwrap().0;
    assert_eq!(account.currency, "INR");
    assert_eq!(account.balance, 1000.0);
}

#[tokio::test]
async fn test_create_user_account_negative_balance() {
    let db_conn = setup_test_db();
    let jwt_auth = JwtAuth::new(b"test_secret");
    let auth_user = create_test_auth_user();

    let request = CreateAccountRequest {
        currency: "INR".to_string(),
        minimum_balance: -100.0,
        country: Some("IN".to_string()),
    };

    let result = create_user_account(State((db_conn, jwt_auth)), auth_user, Json(request)).await;

    assert!(
        result.is_err(),
        "Account creation should fail with negative balance"
    );
    assert!(result.unwrap_err().contains("Validation error"));
}

#[tokio::test]
async fn test_get_current_user_success() {
    let db_conn = setup_test_db();

    // Create a test account for the user since we now return account details
    let user_id = "test_user_id";
    let currency = "INR";
    let balance = 1000.0;

    {
        let conn = db_conn.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO accounts (id, currency, balance, created_at, updated_at)
             VALUES (?1, ?2, ?3, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)",
            [user_id, currency, &balance.to_string()],
        )
        .expect("Failed to create test account");
    }

    let jwt_auth = JwtAuth::new(b"test_secret");
    let auth_user = create_test_auth_user();

    let result = get_current_user(State((db_conn, jwt_auth)), auth_user).await;

    assert!(result.is_ok(), "Getting current user should succeed");
    let account = result.unwrap().0;
    assert_eq!(account.id, "test_user_id");
    assert_eq!(account.currency, currency);
    assert_eq!(account.balance, balance);
}
