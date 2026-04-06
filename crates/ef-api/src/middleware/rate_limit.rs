use axum::{
    Json,
    body::Body,
    extract::ConnectInfo,
    http::{Request, StatusCode},
    middleware::Next,
    response::Response,
};
use governor::{Quota, RateLimiter, clock::DefaultClock, state::keyed::DashMapStateStore};
use std::{net::SocketAddr, num::NonZeroU32, sync::Arc};

use crate::middleware::error_response::ApiError;

/// Keyed rate limiter — one bucket per client IP.
pub type KeyedRateLimiter = RateLimiter<String, DashMapStateStore<String>, DefaultClock>;

/// Build a rate limiter that allows `per_second` requests per second per IP.
pub fn build_rate_limiter(per_second: u32) -> Arc<KeyedRateLimiter> {
    let quota = Quota::per_second(NonZeroU32::new(per_second).expect("rate > 0"));
    Arc::new(RateLimiter::dashmap(quota))
}

/// Axum middleware function that enforces per-IP rate limiting.
pub async fn rate_limit_middleware(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    axum::Extension(limiter): axum::Extension<Arc<KeyedRateLimiter>>,
    request: Request<Body>,
    next: Next,
) -> Result<Response, (StatusCode, Json<ApiError>)> {
    let key = addr.ip().to_string();
    limiter
        .check_key(&key)
        .map_err(|_| ApiError::rate_limited())?;
    Ok(next.run(request).await)
}
