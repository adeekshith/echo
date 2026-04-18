use std::sync::Arc;

use axum::extract::State;
use axum::http::{header, Response, StatusCode};
use axum::body::Body;
use serde::Serialize;

use super::super::errors::AppError;
use crate::state::AppState;

#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
    providers: Vec<ProviderHealth>,
    total_cidrs: usize,
}

#[derive(Serialize)]
struct ProviderHealth {
    provider: String,
    last_synced_at: Option<u64>,
    cidr_count: usize,
    last_error: Option<String>,
}

pub async fn health_handler(
    State(state): State<Arc<AppState>>,
) -> Result<Response<Body>, AppError> {
    let sync_status = state.sync_status.read().await;
    let table = state.lookup_table.read().await;

    let providers: Vec<ProviderHealth> = sync_status
        .iter()
        .map(|s| ProviderHealth {
            provider: s.provider.clone(),
            last_synced_at: s.last_synced_at,
            cidr_count: s.cidr_count,
            last_error: s.last_error.clone(),
        })
        .collect();

    let response = HealthResponse {
        status: if table.is_empty() { "degraded" } else { "ok" },
        total_cidrs: table.len(),
        providers,
    };

    let body = serde_json::to_string_pretty(&response)?;

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body))
        .map_err(|_| AppError::HttpBuilderError)
}
