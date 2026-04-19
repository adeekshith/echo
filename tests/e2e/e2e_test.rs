use std::net::SocketAddr;
use std::sync::Arc;

use tokio::net::TcpListener;
use tokio::sync::RwLock;

use ipecho::config::Config;
use ipecho::lookup::IpLookupTable;
use ipecho::providers::ProviderRecord;
use ipecho::ratelimit::RateLimitState;
use ipecho::routes::create_router;
use ipecho::state::{AppState, SyncStatus};

fn test_config() -> Config {
    Config {
        port: 0,
        sync_interval_secs: 43200,
        log_level: "info".to_string(),
        trusted_proxies: vec!["127.0.0.1/32".parse().unwrap()],
        rate_limit_per_second: 100,
        rate_limit_burst: 100,
        excluded_headers: vec![],
    }
}

async fn start_test_server() -> (String, tokio::task::JoinHandle<()>) {
    let table = IpLookupTable::from_records(vec![ProviderRecord {
        provider: "aws".to_string(),
        cidr: "127.0.0.0/8".to_string(),
        region: Some("us-east-1".to_string()),
        service: Some("AMAZON".to_string()),
    }]);

    let recorder = metrics_exporter_prometheus::PrometheusBuilder::new()
        .build_recorder();
    let handle = recorder.handle();

    let state = AppState {
        lookup_table: Arc::new(RwLock::new(table)),
        sync_status: Arc::new(RwLock::new(vec![SyncStatus {
            provider: "aws".to_string(),
            last_synced_at: Some(1700000000),
            cidr_count: 1,
            last_error: None,
        }])),
        provider_records: Arc::new(RwLock::new(std::collections::HashMap::new())),
        config: Arc::new(test_config()),
        metrics_handle: handle,
    };

    let rl_state = RateLimitState::new(
        state.config.rate_limit_per_second,
        state.config.rate_limit_burst,
    );
    let app = create_router(state, rl_state);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let base_url = format!("http://{}", addr);

    let join_handle = tokio::spawn(async move {
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await
        .unwrap();
    });

    (base_url, join_handle)
}

#[tokio::test]
async fn test_e2e_echo_endpoint() {
    let (base_url, _handle) = start_test_server().await;
    let client = reqwest::Client::new();

    let resp = client
        .get(&base_url)
        .header("user-agent", "e2e-test/1.0")
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.headers().get("content-type").unwrap(),
        "application/json"
    );
    assert_eq!(resp.headers().get("cache-control").unwrap(), "no-store");

    let body = resp.text().await.unwrap();

    // Verify pretty-printed (contains newlines and indentation)
    assert!(body.contains('\n'));
    assert!(body.contains("  "));

    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(json["ip"], "127.0.0.1");
    assert_eq!(json["headers"]["user-agent"], "e2e-test/1.0");
    // 127.0.0.1 matches our seeded 127.0.0.0/8 → aws
    assert_eq!(json["provider"], "aws");
    assert_eq!(json["region"], "us-east-1");
    assert!(json["headers"].is_object());
}

#[tokio::test]
async fn test_e2e_health_endpoint() {
    let (base_url, _handle) = start_test_server().await;
    let client = reqwest::Client::new();

    let resp = client
        .get(format!("{}/health", base_url))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let json: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(json["status"], "ok");
    assert!(json["total_cidrs"].as_u64().unwrap() > 0);
}

#[tokio::test]
async fn test_e2e_metrics_endpoint() {
    let (base_url, _handle) = start_test_server().await;
    let client = reqwest::Client::new();

    let resp = client
        .get(format!("{}/metrics", base_url))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let content_type = resp
        .headers()
        .get("content-type")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(content_type.contains("text/plain"));
}

#[tokio::test]
async fn test_e2e_ip_endpoint() {
    let (base_url, _handle) = start_test_server().await;
    let client = reqwest::Client::new();

    let resp = client.get(format!("{}/ip", base_url)).send().await.unwrap();

    assert_eq!(resp.status(), 200);
    assert_eq!(resp.headers().get("content-type").unwrap(), "text/plain");

    let body = resp.text().await.unwrap();
    assert_eq!(body, "127.0.0.1");
}

#[tokio::test]
async fn test_e2e_provider_endpoint() {
    let (base_url, _handle) = start_test_server().await;
    let client = reqwest::Client::new();

    // 127.0.0.1 matches seeded 127.0.0.0/8 → aws
    let resp = client
        .get(format!("{}/provider", base_url))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let body = resp.text().await.unwrap();
    assert_eq!(body, "aws");
}

#[tokio::test]
async fn test_e2e_region_endpoint() {
    let (base_url, _handle) = start_test_server().await;
    let client = reqwest::Client::new();

    let resp = client
        .get(format!("{}/region", base_url))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let body = resp.text().await.unwrap();
    assert_eq!(body, "us-east-1");
}

#[tokio::test]
async fn test_e2e_headers_endpoint() {
    let (base_url, _handle) = start_test_server().await;
    let client = reqwest::Client::new();

    let resp = client
        .get(format!("{}/headers", base_url))
        .header("x-test", "e2e-value")
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let json: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(json["x-test"], "e2e-value");
}

#[tokio::test]
async fn test_e2e_header_by_name() {
    let (base_url, _handle) = start_test_server().await;
    let client = reqwest::Client::new();

    let resp = client
        .get(format!("{}/headers/x-custom", base_url))
        .header("x-custom", "my-value")
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let body = resp.text().await.unwrap();
    assert_eq!(body, "my-value");
}

#[tokio::test]
async fn test_e2e_header_by_name_missing_returns_404() {
    let (base_url, _handle) = start_test_server().await;
    let client = reqwest::Client::new();

    let resp = client
        .get(format!("{}/headers/x-nonexistent", base_url))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn test_e2e_unknown_path_returns_404() {
    let (base_url, _handle) = start_test_server().await;
    let client = reqwest::Client::new();

    let resp = client
        .get(format!("{}/nonexistent", base_url))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 404);
}
