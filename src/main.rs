mod db;
mod handlers;
mod middleware;
mod models;
mod services;

use crate::middleware::{
    auth::JwtAuth,
    rate_limit::{ip_rate_limiter, IpRateLimiter},
    AuthUser,
};
use axum::{routing::get, Router};
use dotenv::dotenv;
use sqlx::sqlite::SqlitePoolOptions;
use std::{env, net::SocketAddr};
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

    // Configure rate limiters
    // Parse rate limit configuration from environment or use defaults
    let global_rate_limit = env::var("GLOBAL_RATE_LIMIT")
        .map(|v| v.parse::<usize>().unwrap_or(300))
        .unwrap_or(300); // Default: 300 requests per minute globally

    let auth_rate_limit = env::var("AUTH_RATE_LIMIT")
        .map(|v| v.parse::<usize>().unwrap_or(30))
        .unwrap_or(30); // Default: 30 requests per minute for auth endpoints

    // Create IP-based rate limiters for specific endpoints
    let auth_ip_limiter = IpRateLimiter::with_config(auth_rate_limit, 60);
    let global_ip_limiter = IpRateLimiter::with_config(global_rate_limit, 60);

    debug!(
        "Rate limiters configured: global={}/min, auth={}/min",
        global_rate_limit, auth_rate_limit
    );

    // Initialize services
    let exchange_rate_service = services::ExchangeRateService::new(pool.clone());
    let compliance_service = services::ComplianceService::new(pool.clone());

    debug!("Services initialized");

    // Create router with routes
    debug!("Setting up API routes");
    Router::new()
        .route(
            "/health",
            get(|| async {
                debug!("Health check received");
                "OK"
            }),
        )
        .nest(
            "/api",
            Router::new()
                .nest(
                    "/auth",
                    handlers::auth::router()
                        // Apply stricter rate limiting to authentication endpoints
                        .layer(axum::middleware::from_fn_with_state(
                            auth_ip_limiter.clone(),
                            ip_rate_limiter,
                        ))
                        .layer(axum::Extension(auth_ip_limiter)),
                )
                // Protected routes with auth middleware
                .nest(
                    "/protected",
                    Router::new()
                        .nest("/users", handlers::user::router())
                        .nest("/transactions", handlers::transaction::router())
                        .route_layer(axum::middleware::from_extractor::<AuthUser>()),
                )
                .layer(axum::Extension(jwt_auth.clone()))
                .layer(axum::Extension(exchange_rate_service))
                .layer(axum::Extension(compliance_service)),
        )
        // Apply global IP-based rate limiting
        .layer(axum::middleware::from_fn_with_state(
            global_ip_limiter.clone(),
            ip_rate_limiter,
        ))
        .layer(axum::Extension(global_ip_limiter))
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
        .serve(app.into_make_service_with_connect_info::<SocketAddr>())
        .await
    {
        Ok(_) => info!("Server shutdown gracefully"),
        Err(e) => error!("Server error: {}", e),
    }

    Ok(())
}
