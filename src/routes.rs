use std::sync::Arc;

use axum::routing::get;
use axum::Router;
use tower_http::trace::TraceLayer;

use crate::handlers::{echo, health, metrics};
use crate::ratelimit::{RateLimitState, rate_limit_middleware};
use crate::state::AppState;

pub fn create_router(state: AppState) -> Router {
    let rl_state = RateLimitState::new(
        state.config.rate_limit_per_second,
        state.config.rate_limit_burst,
    );
    create_router_with_rate_limiter(state, rl_state)
}

/// Same as [`create_router`] but accepts a prebuilt [`RateLimitState`] so the
/// caller can drive periodic eviction of stale keys (see `main.rs`).
pub fn create_router_with_rate_limiter(state: AppState, rl_state: RateLimitState) -> Router {
    let shared_state = Arc::new(state.clone());

    // Rate-limited routes (public echo endpoints)
    let rate_limited = Router::new()
        .route("/", get(echo::echo_handler))
        .route("/ip", get(echo::ip_handler))
        .route("/provider", get(echo::provider_handler))
        .route("/region", get(echo::region_handler))
        .route("/service", get(echo::service_handler))
        .route("/headers", get(echo::headers_handler))
        .route("/headers/{name}", get(echo::header_by_name_handler))
        .route_layer(axum::middleware::from_fn_with_state(
            rl_state,
            rate_limit_middleware,
        ))
        .with_state(shared_state.clone());

    // Non-rate-limited routes (health, metrics)
    let internal = Router::new()
        .route("/health", get(health::health_handler))
        .route("/metrics", get(metrics::metrics_handler))
        .with_state(shared_state);

    rate_limited
        .merge(internal)
        .layer(TraceLayer::new_for_http())
}
