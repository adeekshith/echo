use std::net::SocketAddr;

use metrics_exporter_prometheus::PrometheusBuilder;
use tokio::net::TcpListener;
use tracing_subscriber::EnvFilter;

mod config;
mod errors;
mod handlers;
mod lookup;
mod providers;
mod ratelimit;
mod routes;
mod state;
mod sync;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = config::Config::from_env();

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new(&config.log_level)),
        )
        .init();

    let metrics_handle = PrometheusBuilder::new()
        .install_recorder()
        .expect("failed to install Prometheus recorder");

    let state = state::AppState::new(config.clone(), metrics_handle);

    let sync_state = state.clone();
    tokio::spawn(async move {
        sync::scheduler::start_sync_loop(sync_state).await;
    });

    let app = routes::create_router(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], config.port));
    tracing::info!("listening on {}", addr);
    let listener = TcpListener::bind(addr).await?;
    axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>()).await?;

    Ok(())
}
