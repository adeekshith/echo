//! HTTP-layer error type.
//!
//! This crate uses a two-tier error model:
//!
//! - **HTTP / handler layer** returns [`AppError`]. Variants map to specific
//!   status codes, so handlers can `?`-propagate cleanly and axum turns them
//!   into JSON responses via [`IntoResponse`].
//! - **Provider / sync / background layer** returns `anyhow::Result`. Errors
//!   there are never matched on — they are logged and surfaced through
//!   `SyncStatus::last_error` as a string, so the extra ceremony of a typed
//!   enum would buy nothing. `anyhow::Context` is used for attaching
//!   human-readable context as errors bubble up.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("JSON serialization failed")]
    JsonError(#[from] serde_json::Error),

    #[error("HTTP builder failed")]
    HttpBuilderError,

    #[error("Header parsing failed")]
    HeaderError(#[from] axum::http::header::ToStrError),

    #[error("Not found: {0}")]
    NotFound(String),
}

impl AppError {
    pub fn status_code(&self) -> StatusCode {
        match self {
            Self::JsonError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::HttpBuilderError => StatusCode::INTERNAL_SERVER_ERROR,
            Self::HeaderError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::NotFound(_) => StatusCode::NOT_FOUND,
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = self.status_code();
        let body = serde_json::json!({
            "error": self.to_string()
        });
        let body_str = serde_json::to_string(&body).unwrap_or_default();
        
        Response::builder()
            .status(status)
            .header("content-type", "application/json")
            .body(axum::body::Body::from(body_str))
            .unwrap_or_else(|_| Response::new(axum::body::Body::from("Internal Server Error")))
    }
}
