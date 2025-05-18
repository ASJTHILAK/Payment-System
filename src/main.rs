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
use tracing::{debug, error, info};

pub async fn create_app() -> Router {
    debug!("Initializing application");

    // Create database connection pool
    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    info!("Using database at: {}", database_url);

    let jwt_secret = env::var("JWT_SECRET").expect("JWT_SECRET must be set");
    debug!("JWT secret loaded");

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await
        .expect("Failed to create pool");

    info!("Database connection pool established");

    // Initialize JWT auth
    let jwt_auth = JwtAuth::new(jwt_secret.as_bytes());
    debug!("JWT authentication initialized");

    // Create router with routes
    debug!("Setting up API routes");
    Router::new()
        .route(
            "/health",
            get(|| async {
                debug!("Health check request received");
                "OK"
            }),
        )
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
    info!("Environment variables loaded");

    // Initialize tracing
    tracing_subscriber::fmt::init();
    info!("Tracing initialized");

    // Create app
    info!("Creating application");
    let app = create_app().await;
    info!("Application created successfully");

    // Start server
    let port = env::var("PORT")
        .unwrap_or_else(|_| {
            debug!("PORT environment variable not set, using default port 3000");
            "3000".to_string()
        })
        .parse::<u16>()?;

    // let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
    let addr = std::net::SocketAddr::from(([0, 0, 0, 0], port)); // Listen on all interfaces to access while running in Docker
    info!("Starting server on {}", addr);

    match axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
    {
        Ok(_) => info!("Server shutdown gracefully"),
        Err(e) => error!("Server error: {}", e),
    }

    Ok(())
}
