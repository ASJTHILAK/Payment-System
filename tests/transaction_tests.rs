use axum::{extract::State, Json};
use payment_system::{
    db::DbPool,
    handlers::transaction::{create, list},
    middleware::{auth::JwtAuth, AuthUser},
    models::CreateTransactionRequest,
};
use sqlx;

// Mock AuthUser for testing
fn create_test_auth_user() -> AuthUser {
    AuthUser {
        user_id: "00000000-0000-0000-0000-000000000000".to_string(),
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

    // Create test users and accounts
    let sender_id = "00000000-0000-0000-0000-000000000000".to_string();
    let recipient_id = "00000000-0000-0000-0000-000000000001".to_string();

    // Create sender
    sqlx::query!(
        r#"
        INSERT INTO users (id, username, email, password_hash, created_at, updated_at)
        VALUES (?, ?, ?, ?, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP),
               (?, ?, ?, ?, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
        "#,
        sender_id,
        "sender",
        "sender@example.com",
        "password_hash",
        recipient_id,
        "recipient",
        "recipient@example.com",
        "password_hash"
    )
    .execute(&pool)
    .await
    .expect("Failed to create test users");

    // Create accounts for both users with INR currency
    sqlx::query!(
        r#"
        INSERT INTO accounts (id, currency, balance, created_at, updated_at)
        VALUES (?, ?, ?, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP),
               (?, ?, ?, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
        "#,
        sender_id,
        "INR",
        5000.0,
        recipient_id,
        "INR",
        1000.0,
    )
    .execute(&pool)
    .await
    .expect("Failed to create test accounts");

    pool
}

#[tokio::test]
async fn test_create_transaction_success() {
    let pool = setup_test_db().await;
    let jwt_auth = JwtAuth::new(b"test_secret");
    let auth_user = create_test_auth_user();
    let recipient_id = "00000000-0000-0000-0000-000000000001";

    let request = CreateTransactionRequest {
        to_account_id: recipient_id.to_string(),
        amount: 1000.0,
        currency: "INR".to_string(),
        description: Some("Test transaction".to_string()),
    };

    let result = create(State((pool.clone(), jwt_auth)), auth_user, Json(request)).await;

    assert!(result.is_ok(), "Transaction creation should succeed");

    let transaction = result.unwrap().0;
    assert_eq!(
        transaction.from_account_id,
        "00000000-0000-0000-0000-000000000000"
    );
    assert_eq!(
        transaction.to_account_id,
        "00000000-0000-0000-0000-000000000001"
    );
    assert_eq!(transaction.amount, 1000.0);
    assert_eq!(transaction.currency, "INR");
    assert_eq!(transaction.description.unwrap(), "Test transaction");
}

#[tokio::test]
async fn test_create_transaction_insufficient_balance() {
    let pool = setup_test_db().await;
    let jwt_auth = JwtAuth::new(b"test_secret");
    let auth_user = create_test_auth_user();
    let recipient_id = "00000000-0000-0000-0000-000000000001";

    let request = CreateTransactionRequest {
        to_account_id: recipient_id.to_string(),
        amount: 10000.0, // Higher than sender's balance
        currency: "INR".to_string(),
        description: None,
    };

    let result = create(State((pool.clone(), jwt_auth)), auth_user, Json(request)).await;

    assert!(
        result.is_err(),
        "Transaction should fail due to insufficient balance"
    );
    assert!(result.unwrap_err().contains("Insufficient balance"));
}

#[tokio::test]
async fn test_create_transaction_invalid_recipient() {
    let pool = setup_test_db().await;
    let jwt_auth = JwtAuth::new(b"test_secret");
    let auth_user = create_test_auth_user();

    let request = CreateTransactionRequest {
        to_account_id: "0000".to_string(),
        amount: 1000.0,
        currency: "INR".to_string(),
        description: None,
    };

    let result = create(State((pool.clone(), jwt_auth)), auth_user, Json(request)).await;

    assert!(
        result.is_err(),
        "Transaction should fail with invalid recipient"
    );
}

#[tokio::test]
async fn test_list_transactions() {
    let pool = setup_test_db().await;
    let jwt_auth = JwtAuth::new(b"test_secret");
    let auth_user = create_test_auth_user();

    // First create a transaction
    let request = CreateTransactionRequest {
        to_account_id: "00000000-0000-0000-0000-000000000001".to_string(),
        amount: 1000.0,
        currency: "INR".to_string(),
        description: Some("Test transaction".to_string()),
    };

    let create_result = create(
        State((pool.clone(), jwt_auth.clone())),
        auth_user.clone(),
        Json(request),
    )
    .await;
    assert!(create_result.is_ok(), "Transaction creation should succeed");

    // Now test listing transactions
    let list_result = list(State((pool.clone(), jwt_auth)), auth_user).await;

    assert!(list_result.is_ok(), "Transaction listing should succeed");
    let transactions = list_result.unwrap().0;
    assert!(
        !transactions.is_empty(),
        "Transaction list should not be empty"
    );

    let last_transaction = transactions.last().unwrap();
    assert_eq!(last_transaction.amount, 1000.0);
    assert_eq!(last_transaction.currency, "INR");
    assert_eq!(
        last_transaction.description.as_ref().unwrap(),
        "Test transaction"
    );
}
