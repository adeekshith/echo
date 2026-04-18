use std::net::SocketAddr;
use std::sync::Arc;

use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use metrics_exporter_prometheus::PrometheusBuilder;
use tokio::sync::RwLock;
use tower::ServiceExt;

use ipecho::config::Config;
use ipecho::lookup::IpLookupTable;
use ipecho::routes::create_router;
use ipecho::state::{AppState, SyncStatus};

fn test_config() -> Config {
    Config {
        port: 8083,
        sync_interval_secs: 43200,
        log_level: "info".to_string(),
        trusted_proxies: vec!["10.0.0.0/8".parse().unwrap()],
        rate_limit_per_second: 1,
        rate_limit_burst: 1,
        excluded_headers: vec![],
    }
}

fn test_metrics_handle() -> metrics_exporter_prometheus::PrometheusHandle {
    let recorder = PrometheusBuilder::new().build_recorder();
    recorder.handle()
}

fn test_state_with_table(table: IpLookupTable) -> AppState {
    AppState {
        lookup_table: Arc::new(RwLock::new(table)),
        sync_status: Arc::new(RwLock::new(vec![])),
        provider_records: Arc::new(RwLock::new(std::collections::HashMap::new())),
        config: Arc::new(test_config()),
        metrics_handle: test_metrics_handle(),
    }
}

#[tokio::test]
async fn test_rate_limit_rejects_over_limit() {
    let state = test_state_with_table(IpLookupTable::empty());
    let app = create_router(state);

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
    let state = test_state_with_table(IpLookupTable::empty());
    let app = create_router(state);

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
    let state = test_state_with_table(IpLookupTable::empty());
    let app = create_router(state);

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
