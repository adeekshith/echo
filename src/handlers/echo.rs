use std::collections::BTreeMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

use axum::extract::{ConnectInfo, State};
use axum::http::header::HeaderMap;
use axum::http::{header, Response, StatusCode};
use axum::body::Body;
use serde::Serialize;

use crate::state::AppState;

#[derive(Debug, Serialize)]
pub struct EchoResponse {
    pub ip: String,
    pub user_agent: Option<String>,
    pub host: Option<String>,
    pub headers: BTreeMap<String, String>,
    pub cloud_provider: Option<String>,
    pub region: Option<String>,
    pub service: Option<String>,
}

pub async fn echo_handler(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Response<Body>, StatusCode> {
    metrics::counter!("http_requests_total", "endpoint" => "/").increment(1);

    let client_ip = extract_client_ip(&addr, &headers, &state.config);

    let user_agent = headers
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    let host = headers
        .get(header::HOST)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    let mut header_map = BTreeMap::new();
    for (name, value) in &headers {
        if let Ok(v) = value.to_str() {
            header_map.insert(name.to_string(), v.to_string());
        }
    }

    let (cloud_provider, region, service) = {
        let table = state.lookup_table.read().await;
        match client_ip.parse::<IpAddr>() {
            Ok(ip) => match table.lookup(ip) {
                Some(entry) => {
                    metrics::counter!("ip_lookup_total", "result" => "hit").increment(1);
                    (
                        Some(entry.provider.clone()),
                        entry.region.clone(),
                        entry.service.clone(),
                    )
                }
                None => {
                    metrics::counter!("ip_lookup_total", "result" => "miss").increment(1);
                    (None, None, None)
                }
            },
            Err(_) => {
                metrics::counter!("ip_lookup_total", "result" => "miss").increment(1);
                (None, None, None)
            }
        }
    };

    let response = EchoResponse {
        ip: client_ip,
        user_agent,
        host,
        headers: header_map,
        cloud_provider,
        region,
        service,
    };

    let body = serde_json::to_string_pretty(&response).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::CACHE_CONTROL, "no-store")
        .body(Body::from(body))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

fn extract_client_ip(addr: &SocketAddr, headers: &HeaderMap, config: &crate::config::Config) -> String {
    let peer_ip = addr.ip();

    if config.is_trusted_proxy(&peer_ip) {
        // Try X-Forwarded-For first (leftmost = original client)
        if let Some(xff) = headers.get("x-forwarded-for").and_then(|v| v.to_str().ok()) {
            if let Some(first_ip) = xff.split(',').next() {
                let trimmed = first_ip.trim();
                if !trimmed.is_empty() {
                    return trimmed.to_string();
                }
            }
        }

        // Try X-Real-IP
        if let Some(real_ip) = headers.get("x-real-ip").and_then(|v| v.to_str().ok()) {
            let trimmed = real_ip.trim();
            if !trimmed.is_empty() {
                return trimmed.to_string();
            }
        }
    }

    peer_ip.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    fn test_config() -> Config {
        Config {
            port: 8083,
            sync_interval_secs: 43200,
            log_level: "info".to_string(),
            trusted_proxies: vec!["10.0.0.0/8".parse().unwrap()],
            rate_limit_per_second: 10,
            rate_limit_burst: 20,
        }
    }

    #[test]
    fn test_extract_ip_direct_connection() {
        let config = test_config();
        let addr: SocketAddr = "203.0.113.1:12345".parse().unwrap();
        let headers = HeaderMap::new();

        let ip = extract_client_ip(&addr, &headers, &config);
        assert_eq!(ip, "203.0.113.1");
    }

    #[test]
    fn test_extract_ip_xff_from_trusted_proxy() {
        let config = test_config();
        let addr: SocketAddr = "10.0.0.1:12345".parse().unwrap();
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", "203.0.113.50, 10.0.0.1".parse().unwrap());

        let ip = extract_client_ip(&addr, &headers, &config);
        assert_eq!(ip, "203.0.113.50");
    }

    #[test]
    fn test_extract_ip_xff_from_untrusted_ignored() {
        let config = test_config();
        let addr: SocketAddr = "203.0.113.1:12345".parse().unwrap();
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", "1.2.3.4".parse().unwrap());

        let ip = extract_client_ip(&addr, &headers, &config);
        assert_eq!(ip, "203.0.113.1");
    }

    #[test]
    fn test_extract_ip_x_real_ip_from_trusted() {
        let config = test_config();
        let addr: SocketAddr = "10.0.0.1:12345".parse().unwrap();
        let mut headers = HeaderMap::new();
        headers.insert("x-real-ip", "203.0.113.99".parse().unwrap());

        let ip = extract_client_ip(&addr, &headers, &config);
        assert_eq!(ip, "203.0.113.99");
    }

    #[test]
    fn test_xff_takes_priority_over_x_real_ip() {
        let config = test_config();
        let addr: SocketAddr = "10.0.0.1:12345".parse().unwrap();
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", "1.1.1.1".parse().unwrap());
        headers.insert("x-real-ip", "2.2.2.2".parse().unwrap());

        let ip = extract_client_ip(&addr, &headers, &config);
        assert_eq!(ip, "1.1.1.1");
    }
}
