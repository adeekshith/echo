use std::net::SocketAddr;

use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

use ipecho::lookup::IpLookupTable;

use super::common::{build_router, test_state_with_table};

#[tokio::test]
async fn test_echo_endpoint_returns_ok() {
    let state = test_state_with_table(IpLookupTable::empty());
    let app = build_router(state);

    let req = Request::builder()
        .uri("/")
        .extension(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 12345))))
        .body(Body::empty())
        .unwrap();

    let response = app.clone().oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_not_found_error_returns_404() {
    let state = test_state_with_table(IpLookupTable::empty());
    let app = build_router(state);

    let req = Request::builder()
        .uri("/headers/x-nonexistent")
        .extension(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 12345))))
        .body(Body::empty())
        .unwrap();

    let response = app.clone().oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_error_response_is_json() {
    let state = test_state_with_table(IpLookupTable::empty());
    let app = build_router(state);

    let req = Request::builder()
        .uri("/headers/x-nonexistent")
        .extension(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 12345))))
        .body(Body::empty())
        .unwrap();

    let response = app.clone().oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert!(response
        .headers()
        .get("content-type")
        .unwrap()
        .to_str()
        .unwrap()
        .contains("application/json"));
}

#[tokio::test]
async fn test_metrics_endpoint_returns_ok() {
    let state = test_state_with_table(IpLookupTable::empty());
    let app = build_router(state);

    let req = Request::builder()
        .uri("/metrics")
        .body(Body::empty())
        .unwrap();

    let response = app.clone().oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert!(response
        .headers()
        .get("content-type")
        .unwrap()
        .to_str()
        .unwrap()
        .contains("text/plain"));
}
