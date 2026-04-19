use axum::body::Body;
use axum::http::{header::HeaderName, HeaderValue, Request, Response};
use axum::middleware::Next;
use tracing::Instrument;
use uuid::Uuid;

/// Header name for request correlation. Lowercase per HTTP/2 rules; axum
/// normalizes regardless, but keeping it lowercase avoids allocations.
const REQUEST_ID_HEADER: HeaderName = HeaderName::from_static("x-request-id");

/// Reuse any inbound `x-request-id` header so callers (e.g. an ingress or
/// upstream proxy) can correlate their traces with ours. Otherwise generate a
/// fresh UUIDv4. The id is attached to both the tracing span (so log lines
/// emitted inside the handler carry it) and the response header.
pub async fn request_id_middleware(
    mut request: Request<Body>,
    next: Next,
) -> Response<Body> {
    let request_id = request
        .headers()
        .get(&REQUEST_ID_HEADER)
        .and_then(|v| v.to_str().ok())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .unwrap_or_else(|| Uuid::new_v4().to_string());

    // Ensure downstream handlers see the id on the request too.
    if let Ok(value) = HeaderValue::from_str(&request_id) {
        request.headers_mut().insert(REQUEST_ID_HEADER, value);
    }

    let span = tracing::info_span!("request", request_id = %request_id);
    let response_id = request_id.clone();

    async move {
        let mut response = next.run(request).await;
        if let Ok(value) = HeaderValue::from_str(&response_id) {
            response.headers_mut().insert(REQUEST_ID_HEADER, value);
        }
        response
    }
    .instrument(span)
    .await
}
