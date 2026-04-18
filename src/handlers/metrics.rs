use std::sync::Arc;

use axum::body::Body;
use axum::extract::State;
use axum::http::{header, Response, StatusCode};

use crate::errors::AppError;
use crate::state::AppState;

pub async fn metrics_handler(
    State(state): State<Arc<AppState>>,
) -> Result<Response<Body>, AppError> {
    let output = state.metrics_handle.render();

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/plain; version=0.0.4")
        .body(Body::from(output))
        .map_err(|_| AppError::HttpBuilderError)
}
