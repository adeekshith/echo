use std::net::SocketAddr;

use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

use ipecho::lookup::IpLookupTable;
use ipecho::providers::ProviderRecord;
use ipecho::state::AppState;

use super::common::{build_router, global_metrics_handle, test_config, test_state};

fn test_state_with_table(table: IpLookupTable) -> AppState {
    // Use the globally installed recorder so metrics::counter! calls made by
    // request handlers actually get captured and show up in /metrics.
    test_state(test_config(), global_metrics_handle(), table)
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
    let app = build_router(state.clone());

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
    let app = build_router(state.clone());

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
    let app = build_router(state.clone());

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
    let mut config = test_config();
    config.rate_limit_per_second = 1;
    config.rate_limit_burst = 1;
    let state = test_state(config, global_metrics_handle(), IpLookupTable::empty());
    let app = build_router(state.clone());

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
