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
use ipecho::providers::ProviderRecord;
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
    PrometheusBuilder::new().install_recorder().unwrap()
}

fn test_state_with_table(table: IpLookupTable) -> AppState {
    AppState {
        lookup_table: Arc::new(RwLock::new(table)),
        sync_status: Arc::new(RwLock::new(vec![])),
        config: Arc::new(test_config()),
        metrics_handle: test_metrics_handle(),
    }
}

fn seeded_lookup_table() -> IpLookupTable {
    IpLookupTable::from_records(vec![
        ProviderRecord {
            provider: "aws".to_string(),
            cidr: "3.0.0.0/8".to_string(),
            region: Some("us-east-1".to_string()),
            service: Some("AMAZON".to_string()),
        },
    ])
}

#[tokio::test]
async fn test_http_requests_counter_incremented() {
    let state = test_state_with_table(IpLookupTable::empty());
    let app = create_router(state.clone());

    let req = Request::builder()
        .uri("/")
        .extension(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 12345))))
        .body(Body::empty())
        .unwrap();

    let response = app.clone().oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let metrics_output = state.metrics_handle.render();
    assert!(metrics_output.contains("http_requests_total"), "Expected http_requests_total in metrics, got: {}", metrics_output);
}

#[tokio::test]
async fn test_ip_lookup_counter_hit() {
    let state = test_state_with_table(seeded_lookup_table());
    let app = create_router(state.clone());

    let req = Request::builder()
        .uri("/")
        .extension(ConnectInfo(SocketAddr::from(([3, 0, 0, 1], 12345))))
        .body(Body::empty())
        .unwrap();

    let response = app.clone().oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let metrics_output = state.metrics_handle.render();
    assert!(metrics_output.contains("ip_lookup_total"), "Expected ip_lookup_total in metrics, got: {}", metrics_output);
}

#[tokio::test]
async fn test_ip_lookup_counter_miss() {
    let state = test_state_with_table(seeded_lookup_table());
    let app = create_router(state.clone());

    let req = Request::builder()
        .uri("/")
        .extension(ConnectInfo(SocketAddr::from(([192, 168, 1, 1], 12345))))
        .body(Body::empty())
        .unwrap();

    let response = app.clone().oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let metrics_output = state.metrics_handle.render();
    assert!(metrics_output.contains("ip_lookup_total"), "Expected ip_lookup_total in metrics, got: {}", metrics_output);
}

#[tokio::test]
async fn test_rate_limit_rejected_counter_incremented() {
    let config = Config {
        rate_limit_per_second: 1,
        rate_limit_burst: 1,
        ..test_config()
    };
    let state = AppState {
        lookup_table: Arc::new(RwLock::new(IpLookupTable::empty())),
        sync_status: Arc::new(RwLock::new(vec![])),
        config: Arc::new(config),
        metrics_handle: test_metrics_handle(),
    };
    let app = create_router(state.clone());

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

    let metrics_output = state.metrics_handle.render();
    assert!(metrics_output.contains("rate_limit_rejected_total"), "Expected rate_limit_rejected_total in metrics, got: {}", metrics_output);
}
