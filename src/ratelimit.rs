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

use crate::errors::AppError;

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

    /// Evict keys whose rate-limit state has fully replenished. Without this,
    /// the DashMap grows by one entry per unique client IP and never shrinks,
    /// so long-running instances slowly leak memory.
    pub fn retain_recent(&self) {
        self.limiter.retain_recent();
        self.limiter.shrink_to_fit();
    }

    /// Current number of tracked IPs. Used for observability of the eviction
    /// loop; governor may return an estimate depending on the store.
    pub fn tracked_ip_count(&self) -> usize {
        self.limiter.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn tracked_ip_count_reflects_checked_keys() {
        let rl = RateLimitState::new(100, 100);
        assert_eq!(rl.tracked_ip_count(), 0);

        let a: IpAddr = "10.0.0.1".parse().unwrap();
        let b: IpAddr = "10.0.0.2".parse().unwrap();
        let _ = rl.limiter.check_key(&a);
        let _ = rl.limiter.check_key(&b);
        let _ = rl.limiter.check_key(&a); // same key again — still one tracked entry

        assert_eq!(rl.tracked_ip_count(), 2);
    }

    #[test]
    fn retain_recent_drops_replenished_keys() {
        // At 1000 rps the GCRA interval is 1ms, so after sleeping 100ms a
        // just-checked key's theoretical arrival time is firmly in the past
        // and retain_recent() considers it indistinguishable from fresh.
        let rl = RateLimitState::new(1000, 1);
        let ip: IpAddr = "203.0.113.7".parse().unwrap();
        let _ = rl.limiter.check_key(&ip);
        assert_eq!(rl.tracked_ip_count(), 1);

        std::thread::sleep(Duration::from_millis(100));
        rl.retain_recent();

        assert_eq!(
            rl.tracked_ip_count(),
            0,
            "key with fully-replenished quota should be evicted"
        );
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
