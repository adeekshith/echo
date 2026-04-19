use std::net::SocketAddr;

use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt;

use ipecho::lookup::IpLookupTable;
use ipecho::state::AppState;

use super::common::{build_router, test_config, test_state, throwaway_metrics_handle};

/// Rate-limit-specific state: same as [`common::test_state_with_table`] but
/// with a 1-rps/1-burst limiter so tests can observe rejections after a
/// single request.
fn strict_rate_limit_state(table: IpLookupTable) -> AppState {
    let mut config = test_config();
    config.rate_limit_per_second = 1;
    config.rate_limit_burst = 1;
    test_state(config, throwaway_metrics_handle(), table)
}

#[tokio::test]
async fn test_rate_limit_rejects_over_limit() {
    let state = strict_rate_limit_state(IpLookupTable::empty());
    let app = build_router(state);

    let addr = SocketAddr::from(([127, 0, 0, 1], 12345));

    let req = Request::builder()
        .uri("/")
        .extension(ConnectInfo(addr))
        .body(Body::empty())
        .unwrap();

    let response = app.clone().oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let req = Request::builder()
        .uri("/")
        .extension(ConnectInfo(addr))
        .body(Body::empty())
        .unwrap();

    let response = app.clone().oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
}

#[tokio::test]
async fn test_rate_limit_is_per_ip() {
    let state = strict_rate_limit_state(IpLookupTable::empty());
    let app = build_router(state);

    let addr1 = SocketAddr::from(([127, 0, 0, 1], 12345));
    let addr2 = SocketAddr::from(([127, 0, 0, 2], 12345));

    let req = Request::builder()
        .uri("/")
        .extension(ConnectInfo(addr1))
        .body(Body::empty())
        .unwrap();

    let response = app.clone().oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let req = Request::builder()
        .uri("/")
        .extension(ConnectInfo(addr2))
        .body(Body::empty())
        .unwrap();

    let response = app.clone().oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_rate_limit_returns_json_error() {
    let state = strict_rate_limit_state(IpLookupTable::empty());
    let app = build_router(state);

    let addr = SocketAddr::from(([127, 0, 0, 1], 12345));

    let req = Request::builder()
        .uri("/")
        .extension(ConnectInfo(addr))
        .body(Body::empty())
        .unwrap();

    let response = app.clone().oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let req = Request::builder()
        .uri("/")
        .extension(ConnectInfo(addr))
        .body(Body::empty())
        .unwrap();

    let response = app.clone().oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    assert!(response
        .headers()
        .get("content-type")
        .unwrap()
        .to_str()
        .unwrap()
        .contains("application/json"));

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body_str = String::from_utf8(body.to_vec()).unwrap();
    assert!(body_str.contains("Too Many Requests"));
}
