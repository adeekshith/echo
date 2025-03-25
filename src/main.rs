use axum::{extract::ConnectInfo, response::IntoResponse, routing::get, Router};
use std::net::{Ipv6Addr, SocketAddr};
use tokio::net::TcpSocket;

#[tokio::main]
async fn main() {
    let app = Router::new().route("/", get(handler));

    // Use a dual-stack socket that accepts both IPv4 and IPv6.
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

// Handler function that extracts the client's IP address.
async fn handler(ConnectInfo(addr): ConnectInfo<SocketAddr>) -> impl IntoResponse {
    format!("client_ip: {}\n", addr.ip()) // Ensures response ends with newline
}
