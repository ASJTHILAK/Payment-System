use axum::{extract::State, Json};
use payment_system::{
    db::DbPool,
    handlers::user::{create_user_account, get_current_user},
    middleware::{auth::JwtAuth, AuthUser},
    models::CreateAccountRequest,
};
use sqlx;

// Mock AuthUser for testing
fn create_test_auth_user() -> AuthUser {
    AuthUser {
        user_id: "test_user_id".to_string(),
    }
}

// Mock DbPool for testing
async fn setup_test_db() -> DbPool {
    let db_url = "sqlite::memory:";
    let pool = sqlx::SqlitePool::connect(db_url)
        .await
        .expect("Failed to create test database");

    // Run migrations
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("Failed to run migrations");

    // Create a test user since accounts are linked to users
    let user_id = "test_user_id".to_string();
    sqlx::query!(
        r#"
        INSERT INTO users (id, username, email, password_hash, created_at, updated_at)
        VALUES (?, ?, ?, ?, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
        "#,
        user_id,
        "testuser",
        "test@example.com",
        "password_hash"
    )
    .execute(&pool)
    .await
    .expect("Failed to create test user");

    pool
}

#[tokio::test]
async fn test_create_user_account_success() {
    let pool = setup_test_db().await;
    let jwt_auth = JwtAuth::new(b"test_secret");
    let auth_user = create_test_auth_user();

    let request = CreateAccountRequest {
        currency: "INR".to_string(),
        minimum_balance: 1000.0,
        country: Some("IN".to_string()),
    };

    let result = create_user_account(State((pool, jwt_auth)), auth_user, Json(request)).await;

    assert!(result.is_ok(), "Account creation should succeed");

    let account = result.unwrap().0;
    assert_eq!(account.currency, "INR");
    assert_eq!(account.balance, 1000.0);
}

#[tokio::test]
async fn test_create_user_account_negative_balance() {
    let pool = setup_test_db().await;
    let jwt_auth = JwtAuth::new(b"test_secret");
    let auth_user = create_test_auth_user();

    let request = CreateAccountRequest {
        currency: "INR".to_string(),
        minimum_balance: -100.0,
        country: Some("IN".to_string()),
    };

    let result = create_user_account(State((pool, jwt_auth)), auth_user, Json(request)).await;

    assert!(
        result.is_err(),
        "Account creation should fail with negative balance"
    );
    assert!(result.unwrap_err().contains("Validation error"));
}

#[tokio::test]
async fn test_get_current_user_success() {
    let pool = setup_test_db().await;

    // Create a test account for the user since we now return account details
    let user_id = "test_user_id".to_string();
    let currency = "INR";
    let balance = 1000.0;

    sqlx::query!(
        r#"
        INSERT INTO accounts (id, currency, balance, created_at, updated_at)
        VALUES (?, ?, ?, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
        "#,
        user_id,
        currency,
        balance
    )
    .execute(&pool)
    .await
    .expect("Failed to create test account");

    let jwt_auth = JwtAuth::new(b"test_secret");
    let auth_user = create_test_auth_user();

    let result = get_current_user(State((pool, jwt_auth)), auth_user).await;

    assert!(result.is_ok(), "Getting current user should succeed");
    let account = result.unwrap().0;
    assert_eq!(account.id, "test_user_id");
    assert_eq!(account.currency, currency);
    assert_eq!(account.balance, balance);
}
