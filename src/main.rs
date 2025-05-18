mod db;
mod handlers;
mod middleware;
mod models;

use crate::middleware::{auth::JwtAuth, AuthUser};
use axum::{routing::get, Router};
use dotenv::dotenv;
use sqlx::sqlite::SqlitePoolOptions;
use std::env;
use tower_http::trace::TraceLayer;

pub async fn create_app() -> Router {
    // Create database connection pool
    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let jwt_secret = env::var("JWT_SECRET").expect("JWT_SECRET must be set");

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await
        .expect("Failed to create pool");

    // Initialize JWT auth
    let jwt_auth = JwtAuth::new(jwt_secret.as_bytes());

    // Create router with routes
    Router::new()
        .route("/health", get(|| async { "OK" }))
        .nest(
            "/api",
            Router::new()
                .nest("/auth", handlers::auth::router())
                // Protected routes with auth middleware
                .nest(
                    "/protected",
                    Router::new()
                        .nest("/users", handlers::user::router())
                        .nest("/transactions", handlers::transaction::router())
                        .route_layer(axum::middleware::from_extractor::<AuthUser>()),
                )
                .layer(axum::Extension(jwt_auth.clone())),
        )
        .layer(TraceLayer::new_for_http())
        .with_state((pool, jwt_auth.clone()))
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load environment variables
    dotenv().ok();

    // Initialize tracing
    tracing_subscriber::fmt::init();

    // Create app
    let app = create_app().await;

    // Start server
    let port = env::var("PORT")
        .unwrap_or_else(|_| "3000".to_string())
        .parse::<u16>()?;
    // let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
    let addr = std::net::SocketAddr::from(([0, 0, 0, 0], port)); // Listen on all interfaces to access while running in Docker
    tracing::info!("Starting server on {}", addr);

    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await?;

    Ok(())
}
