use axum::{
    extract::ConnectInfo,
    http::{Request, StatusCode},
    middleware::Next,
    response::Response,
};
use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

/// Holds information about request rate
#[derive(Debug, Clone)]
pub struct RateLimitState {
    /// When the bucket was first created
    pub start_time: Instant,
    /// Number of requests in the current window
    pub request_count: usize,
}

/// Rate limiter configuration
#[derive(Debug, Clone)]
pub struct RateLimiterConfig {
    /// Maximum requests allowed in the time window
    pub max_requests: usize,
    /// Time window in seconds
    pub window_size: u64,
}

impl Default for RateLimiterConfig {
    fn default() -> Self {
        Self {
            max_requests: 100,
            window_size: 60, // 1 minute
        }
    }
}

/// Stores rate limiting data by IP address
#[derive(Debug, Clone)]
pub struct IpRateLimiter {
    pub config: RateLimiterConfig,
    pub state: Arc<Mutex<HashMap<String, RateLimitState>>>,
}

impl IpRateLimiter {
    pub fn new(config: RateLimiterConfig) -> Self {
        Self {
            config,
            state: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn with_config(max_requests: usize, window_size: u64) -> Self {
        Self::new(RateLimiterConfig {
            max_requests,
            window_size,
        })
    }
}

/// Middleware function for IP-based rate limiting
pub async fn ip_rate_limiter<B>(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    limiter: axum::extract::Extension<IpRateLimiter>,
    req: Request<B>,
    next: Next<B>,
) -> Response {
    let ip = addr.ip().to_string();
    let now = Instant::now();
    let window_duration = Duration::from_secs(limiter.config.window_size);

    // Check if the request should be rate limited
    let should_allow_request = {
        let mut state = limiter.state.lock().unwrap();

        if let Some(bucket) = state.get_mut(&ip) {
            // If the window has expired, reset the bucket
            if now.duration_since(bucket.start_time) > window_duration {
                bucket.start_time = now;
                bucket.request_count = 1;
                true
            } else if bucket.request_count < limiter.config.max_requests {
                // If under the limit, increment and allow
                bucket.request_count += 1;
                true
            } else {
                // Over the limit
                false
            }
        } else {
            // First request for this IP
            state.insert(
                ip.clone(),
                RateLimitState {
                    start_time: now,
                    request_count: 1,
                },
            );
            true
        }
    };

    if should_allow_request {
        // Add rate limit headers to successful responses too
        let mut response = next.run(req).await;

        // We already have the lock released, so it's safe to lock again
        let state = limiter.state.lock().unwrap();
        if let Some(bucket) = state.get(&ip) {
            let headers = response.headers_mut();

            // Add rate limit headers (standard practice)
            headers.insert("X-RateLimit-Limit", limiter.config.max_requests.into());
            headers.insert(
                "X-RateLimit-Remaining",
                (limiter
                    .config
                    .max_requests
                    .saturating_sub(bucket.request_count))
                .into(),
            );

            // Add reset time in seconds
            let reset_time = window_duration
                .checked_sub(now.duration_since(bucket.start_time))
                .unwrap_or(Duration::from_secs(0))
                .as_secs();
            headers.insert("X-RateLimit-Reset", reset_time.into());
        }

        response
    } else {
        tracing::warn!("Rate limit exceeded for IP: {}", addr.ip());
        let retry_after = window_duration.as_secs();

        Response::builder()
            .status(StatusCode::TOO_MANY_REQUESTS)
            .header("Content-Type", "application/json")
            .header("Retry-After", retry_after.to_string())
            .header("X-RateLimit-Limit", limiter.config.max_requests.to_string())
            .header("X-RateLimit-Remaining", "0")
            .header("X-RateLimit-Reset", retry_after.to_string())
            .body(axum::body::boxed(axum::body::Full::from(format!(
                "{{\"error\":\"Too many requests\",\"retry_after\":{}}}",
                retry_after
            ))))
            .unwrap()
    }
}

// Note: This module provides IP-based rate limiting that is applied at both
// global and route-specific levels in the application.
