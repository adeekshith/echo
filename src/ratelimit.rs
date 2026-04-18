use std::net::IpAddr;
use std::num::NonZeroU32;
use std::sync::Arc;

use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, Response, StatusCode};
use axum::middleware::Next;
use governor::clock::DefaultClock;
use governor::state::keyed::DashMapStateStore;
use governor::{Quota, RateLimiter};

use super::errors::AppError;

type Limiter = RateLimiter<IpAddr, DashMapStateStore<IpAddr>, DefaultClock>;

#[derive(Clone)]
pub struct RateLimitState {
    limiter: Arc<Limiter>,
}

impl RateLimitState {
    pub fn new(per_second: u64, burst: u32) -> Self {
        let quota = Quota::per_second(NonZeroU32::new(per_second as u32).unwrap_or(NonZeroU32::MIN))
            .allow_burst(NonZeroU32::new(burst).unwrap_or(NonZeroU32::MIN));
        let limiter = Arc::new(RateLimiter::keyed(quota));
        Self { limiter }
    }
}

pub async fn rate_limit_middleware(
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    axum::extract::State(rl): axum::extract::State<RateLimitState>,
    request: Request<Body>,
    next: Next,
) -> Result<Response<Body>, AppError> {
    let ip = addr.ip();

    match rl.limiter.check_key(&ip) {
        Ok(_) => Ok(next.run(request).await),
        Err(_) => {
            metrics::counter!("rate_limit_rejected_total").increment(1);
            let body = serde_json::json!({
                "error": "Too Many Requests",
                "message": "Rate limit exceeded. Please try again later."
            });
            Response::builder()
                .status(StatusCode::TOO_MANY_REQUESTS)
                .header("content-type", "application/json")
                .header("retry-after", "1")
                .body(Body::from(serde_json::to_string_pretty(&body)?))
                .map_err(|_| AppError::HttpBuilderError)
        }
    }
}
