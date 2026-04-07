use std::sync::Arc;

use axum::routing::get;
use axum::Router;
use tower_http::trace::TraceLayer;

use crate::handlers::{echo, health, metrics};
use crate::state::AppState;

pub fn create_router(state: AppState) -> Router {
    let shared_state = Arc::new(state);

    Router::new()
        .route("/", get(echo::echo_handler))
        .route("/health", get(health::health_handler))
        .route("/metrics", get(metrics::metrics_handler))
        .layer(TraceLayer::new_for_http())
        .with_state(shared_state)
}
