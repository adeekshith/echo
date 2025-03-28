use axum::body::Body;
use axum::{
    extract::ConnectInfo, http::Request, response::IntoResponse, routing::get, Json, Router,
};
use serde::Serialize;
use std::net::{Ipv6Addr, SocketAddr};
use tokio::net::TcpSocket;

#[derive(Serialize)]
struct RequestInfo {
    ip_addr: String,
    remote_host: String,
    user_agent: Option<String>,
    port: u16,
    method: String,
    encoding: Option<String>,
    mime: Option<String>,
    language: Option<String>,
    referer: Option<String>,
    connection: Option<String>,
    keep_alive: Option<String>,
    charset: Option<String>,
    via: Option<String>,
    forwarded: Option<String>,
}

/// Extract all desired information from the request.
fn get_request_info(addr: SocketAddr, req: &Request<Body>) -> RequestInfo {
    let headers = req.headers();
    RequestInfo {
        ip_addr: addr.ip().to_string(),
        // Reverse lookup for remote host isn't performed.
        remote_host: "unavailable".to_string(),
        user_agent: headers
            .get("user-agent")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string()),
        port: addr.port(),
        method: req.method().to_string(),
        encoding: headers
            .get("accept-encoding")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string()),
        mime: headers
            .get("accept")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string()),
        language: headers
            .get("accept-language")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string()),
        referer: headers
            .get("referer")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string()),
        connection: headers
            .get("connection")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string()),
        keep_alive: headers
            .get("keep-alive")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string()),
        charset: headers
            .get("accept-charset")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string()),
        via: headers
            .get("via")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string()),
        forwarded: headers
            .get("forwarded")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string()),
    }
}

/// Returns only the client's IP address.
async fn ip_handler(ConnectInfo(addr): ConnectInfo<SocketAddr>) -> impl IntoResponse {
    format!("{}\n", addr.ip())
}

/// Returns the User-Agent header.
async fn ua_handler(
    ConnectInfo(_addr): ConnectInfo<SocketAddr>,
    req: Request<Body>,
) -> impl IntoResponse {
    let user_agent = req
        .headers()
        .get("user-agent")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown");
    format!("{}\n", user_agent)
}

/// Returns the Accept-Language header.
async fn lang_handler(
    ConnectInfo(_addr): ConnectInfo<SocketAddr>,
    req: Request<Body>,
) -> impl IntoResponse {
    let lang = req
        .headers()
        .get("accept-language")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown");
    format!("{}\n", lang)
}

/// Returns the Accept-Encoding header.
async fn encoding_handler(
    ConnectInfo(_addr): ConnectInfo<SocketAddr>,
    req: Request<Body>,
) -> impl IntoResponse {
    let encoding = req
        .headers()
        .get("accept-encoding")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown");
    format!("{}\n", encoding)
}

/// Returns the Accept header (mime types).
async fn mime_handler(
    ConnectInfo(_addr): ConnectInfo<SocketAddr>,
    req: Request<Body>,
) -> impl IntoResponse {
    let mime = req
        .headers()
        .get("accept")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown");
    format!("{}\n", mime)
}

/// Returns the Forwarded header.
async fn forwarded_handler(
    ConnectInfo(_addr): ConnectInfo<SocketAddr>,
    req: Request<Body>,
) -> impl IntoResponse {
    let forwarded = req
        .headers()
        .get("forwarded")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown");
    format!("{}\n", forwarded)
}

/// Returns all information in plain text.
async fn all_handler(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    req: Request<Body>,
) -> impl IntoResponse {
    let info = get_request_info(addr, &req);
    format!(
        "ip_addr: {}\nremote_host: {}\nuser_agent: {}\nport: {}\nlanguage: {}\nreferer: {}\nconnection: {}\nkeep_alive: {}\nmethod: {}\nencoding: {}\nmime: {}\ncharset: {}\nvia: {}\nforwarded: {}\n",
        info.ip_addr,
        info.remote_host,
        info.user_agent.unwrap_or_default(),
        info.port,
        info.language.unwrap_or_default(),
        info.referer.unwrap_or_default(),
        info.connection.unwrap_or_default(),
        info.keep_alive.unwrap_or_default(),
        info.method,
        info.encoding.unwrap_or_default(),
        info.mime.unwrap_or_default(),
        info.charset.unwrap_or_default(),
        info.via.unwrap_or_default(),
        info.forwarded.unwrap_or_default(),
    )
}

/// Returns all information as JSON.
async fn all_json_handler(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    req: Request<Body>,
) -> impl IntoResponse {
    let info = get_request_info(addr, &req);
    Json(info)
}

#[tokio::main]
async fn main() {
    // Build the application with the routes.
    let app = Router::new()
        .route("/", get(ip_handler))
        .route("/ip", get(ip_handler))
        .route("/ua", get(ua_handler))
        .route("/lang", get(lang_handler))
        .route("/encoding", get(encoding_handler))
        .route("/mime", get(mime_handler))
        .route("/forwarded", get(forwarded_handler))
        .route("/all", get(all_handler))
        .route("/all.json", get(all_json_handler));

    // Use a dual-stack socket (IPv4 & IPv6) listening on port 80.
    let dual_addr = SocketAddr::from((Ipv6Addr::UNSPECIFIED, 80));
    println!("Listening on dual-stack address: {}", dual_addr);

    let socket = TcpSocket::new_v6().expect("failed to create IPv6 socket");
    socket
        .bind(dual_addr)
        .expect("failed to bind to dual-stack address");
    let listener = socket
        .listen(1024)
        .expect("failed to listen on dual-stack socket");

    println!("Dual-stack server running on {}", dual_addr);

    axum::Server::from_tcp(listener.into_std().unwrap())
        .unwrap()
        .serve(app.into_make_service_with_connect_info::<SocketAddr>())
        .await
        .unwrap();
}
