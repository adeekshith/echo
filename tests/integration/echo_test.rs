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
        ProviderRecord {
            provider: "gcp".to_string(),
            cidr: "34.0.0.0/8".to_string(),
            region: Some("us-central1".to_string()),
            service: Some("Google Cloud".to_string()),
        },
    ])
}

fn test_metrics_handle() -> metrics_exporter_prometheus::PrometheusHandle {
    let recorder = PrometheusBuilder::new().build_recorder();
    recorder.handle()
}

fn test_state_with_table(table: IpLookupTable) -> AppState {
    AppState {
        lookup_table: Arc::new(RwLock::new(table)),
        sync_status: Arc::new(RwLock::new(vec![SyncStatus {
            provider: "aws".to_string(),
            last_synced_at: Some(1700000000),
            cidr_count: 1,
            last_error: None,
        }])),
        config: Arc::new(test_config()),
        metrics_handle: test_metrics_handle(),
    }
}

#[tokio::test]
async fn test_echo_returns_json_with_headers() {
    let state = test_state_with_table(IpLookupTable::empty());
    let app = create_router(state);

    let req = Request::builder()
        .uri("/")
        .header("user-agent", "test-agent/1.0")
        .header("host", "localhost:8083")
        .extension(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 12345))))
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(req).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get("content-type").unwrap(),
        "application/json"
    );
    assert_eq!(
        response.headers().get("cache-control").unwrap(),
        "no-store"
    );

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["ip"], "127.0.0.1");
    assert_eq!(json["user_agent"], "test-agent/1.0");
    assert!(json["cloud_provider"].is_null());
    assert!(json["region"].is_null());
    assert!(json["headers"].is_object());
}

#[tokio::test]
async fn test_echo_with_cloud_provider_match() {
    let state = test_state_with_table(seeded_lookup_table());
    let app = create_router(state);

    let req = Request::builder()
        .uri("/")
        .extension(ConnectInfo(SocketAddr::from(([3, 0, 0, 1], 12345))))
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["ip"], "3.0.0.1");
    assert_eq!(json["cloud_provider"], "aws");
    assert_eq!(json["region"], "us-east-1");
    assert_eq!(json["service"], "AMAZON");
}

#[tokio::test]
async fn test_echo_pretty_prints_json() {
    let state = test_state_with_table(IpLookupTable::empty());
    let app = create_router(state);

    let req = Request::builder()
        .uri("/")
        .extension(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 12345))))
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body_str = String::from_utf8(body.to_vec()).unwrap();

    assert!(body_str.contains('\n'));
    assert!(body_str.contains("  "));
}

#[tokio::test]
async fn test_health_endpoint() {
    let state = test_state_with_table(seeded_lookup_table());
    let app = create_router(state);

    let req = Request::builder()
        .uri("/health")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["status"], "ok");
    assert!(json["total_cidrs"].as_u64().unwrap() > 0);
    assert!(json["providers"].is_array());
}

#[tokio::test]
async fn test_health_degraded_when_empty() {
    let state = test_state_with_table(IpLookupTable::empty());
    let app = create_router(state);

    let req = Request::builder()
        .uri("/health")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["status"], "degraded");
}

#[tokio::test]
async fn test_metrics_endpoint() {
    let state = test_state_with_table(IpLookupTable::empty());
    let app = create_router(state);

    let req = Request::builder()
        .uri("/metrics")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert!(response
        .headers()
        .get("content-type")
        .unwrap()
        .to_str()
        .unwrap()
        .contains("text/plain"));
}
