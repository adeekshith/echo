use std::collections::BTreeMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

use axum::extract::{ConnectInfo, Path, State};
use axum::http::header::HeaderMap;
use axum::http::{header, Response, StatusCode};
use axum::body::Body;
use serde::Serialize;

use crate::errors::AppError;
use crate::state::AppState;

#[derive(Debug, Serialize)]
pub struct EchoResponse {
    pub ip: String,
    pub provider: Option<String>,
    pub region: Option<String>,
    pub service: Option<String>,
    pub headers: BTreeMap<String, String>,
}

struct EchoData {
    ip: String,
    provider: Option<String>,
    region: Option<String>,
    service: Option<String>,
    headers: BTreeMap<String, String>,
}

async fn build_echo_data(
    addr: &SocketAddr,
    headers: &HeaderMap,
    state: &AppState,
) -> EchoData {
    let client_ip = extract_client_ip(addr, headers, &state.config);

    let header_map = filter_headers(headers, &state.config.excluded_headers);

    let (provider, region, service) = {
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

    EchoData {
        ip: client_ip,
        provider,
        region,
        service,
        headers: header_map,
    }
}

fn filter_headers(headers: &HeaderMap, excluded: &[String]) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    for (name, value) in headers {
        let name_str = name.as_str();
        if excluded.iter().any(|e| e == name_str) {
            continue;
        }
        if let Ok(v) = value.to_str() {
            map.insert(name_str.to_string(), v.to_string());
        }
    }
    map
}

fn plain_text_response(body: String) -> Result<Response<Body>, AppError> {
Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/plain")
        .header(header::CACHE_CONTROL, "no-store")
        .body(Body::from(body))
        .map_err(|_| AppError::HttpBuilderError)
}

fn optional_plain_text_response(value: Option<String>) -> Result<Response<Body>, AppError> {
    match value {
        Some(v) => plain_text_response(v),
        None => Response::builder()
            .status(StatusCode::NO_CONTENT)
            .header(header::CACHE_CONTROL, "no-store")
            .body(Body::empty())
            .map_err(|_| AppError::HttpBuilderError),
    }
}

// GET / — full JSON response
pub async fn echo_handler(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Response<Body>, AppError> {
    metrics::counter!("http_requests_total", "endpoint" => "/").increment(1);

    let data = build_echo_data(&addr, &headers, &state).await;

    let response = EchoResponse {
        ip: data.ip,
        provider: data.provider,
        region: data.region,
        service: data.service,
        headers: data.headers,
    };

    let body = serde_json::to_string_pretty(&response)?;

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::CACHE_CONTROL, "no-store")
        .body(Body::from(body))
        .map_err(|_| AppError::HttpBuilderError)
}

// GET /ip — plain text IP address
pub async fn ip_handler(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Response<Body>, AppError> {
    metrics::counter!("http_requests_total", "endpoint" => "/ip").increment(1);
    let ip = extract_client_ip(&addr, &headers, &state.config);
    plain_text_response(ip)
}

// GET /provider — plain text provider or 204
pub async fn provider_handler(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Response<Body>, AppError> {
    metrics::counter!("http_requests_total", "endpoint" => "/provider").increment(1);
    let data = build_echo_data(&addr, &headers, &state).await;
    optional_plain_text_response(data.provider)
}

// GET /region — plain text region or 204
pub async fn region_handler(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Response<Body>, AppError> {
    metrics::counter!("http_requests_total", "endpoint" => "/region").increment(1);
    let data = build_echo_data(&addr, &headers, &state).await;
    optional_plain_text_response(data.region)
}

// GET /service — plain text service or 204
pub async fn service_handler(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Response<Body>, AppError> {
    metrics::counter!("http_requests_total", "endpoint" => "/service").increment(1);
    let data = build_echo_data(&addr, &headers, &state).await;
    optional_plain_text_response(data.service)
}

// GET /headers — pretty JSON of all headers
pub async fn headers_handler(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Response<Body>, AppError> {
    metrics::counter!("http_requests_total", "endpoint" => "/headers").increment(1);

    let header_map = filter_headers(&headers, &state.config.excluded_headers);

let body = serde_json::to_string_pretty(&header_map)?;

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::CACHE_CONTROL, "no-store")
        .body(Body::from(body))
        .map_err(|_| AppError::HttpBuilderError)
}

// GET /headers/:name — single header value or 404
pub async fn header_by_name_handler(
    Path(name): Path<String>,
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Response<Body>, AppError> {
    metrics::counter!("http_requests_total", "endpoint" => "/headers/{name}").increment(1);

    let name_lower = name.to_lowercase();

    if state.config.is_header_excluded(&name_lower) {
        return Err(AppError::NotFound("header not found".to_string()));
    }

    for (key, value) in &headers {
        if key.as_str() == name_lower {
            if let Ok(v) = value.to_str() {
                return plain_text_response(v.to_string());
            }
        }
    }

    Err(AppError::NotFound("header not found".to_string()))
}

fn extract_client_ip(
    addr: &SocketAddr,
    headers: &HeaderMap,
    config: &crate::config::Config,
) -> String {
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
            excluded_headers: vec![],
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
