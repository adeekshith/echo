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
use ipecho::errors::AppError;
use ipecho::lookup::IpLookupTable;
use ipecho::routes::create_router;
use ipecho::state::{AppState, SyncStatus};

fn test_config() -> Config {
    Config {
        port: 8083,
        sync_interval_secs: 43200,
        log_level: "info".to_string(),
        trusted_proxies: vec!["10.0.0.0/8".parse().unwrap()],
        rate_limit_per_second: 100,
        rate_limit_burst: 100,
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
        config: Arc::new(test_config()),
        metrics_handle: test_metrics_handle(),
    }
}

#[tokio::test]
async fn test_echo_endpoint_returns_ok() {
    let state = test_state_with_table(IpLookupTable::empty());
    let app = create_router(state);

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
    let app = create_router(state);

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
    let app = create_router(state);

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
    let app = create_router(state);

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
