use std::net::SocketAddr;
use std::time::Duration;

use metrics_exporter_prometheus::PrometheusBuilder;
use tokio::net::TcpListener;
use tracing_subscriber::EnvFilter;

mod config;
mod errors;
mod handlers;
mod lookup;
mod providers;
mod ratelimit;
mod request_id;
mod routes;
mod state;
mod sync;

/// How often the rate limiter sweeps idle IPs out of its DashMap. 60s is a
/// balance between memory pressure (many short-lived clients) and doing
/// unnecessary work under steady traffic.
const RATE_LIMIT_EVICTION_INTERVAL: Duration = Duration::from_secs(60);

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = config::Config::from_env()
        .map_err(|e| anyhow::anyhow!("invalid configuration: {e}"))?;

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

    let rl_state = ratelimit::RateLimitState::new(
        state.config.rate_limit_per_second,
        state.config.rate_limit_burst,
    );

    let eviction_rl = rl_state.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(RATE_LIMIT_EVICTION_INTERVAL);
        loop {
            interval.tick().await;
            eviction_rl.retain_recent();
            metrics::gauge!("rate_limit_tracked_ips")
                .set(eviction_rl.tracked_ip_count() as f64);
        }
    });

    let app = routes::create_router_with_rate_limiter(state, rl_state);

    let addr = SocketAddr::from(([0, 0, 0, 0], config.port));
    tracing::info!("listening on {}", addr);
    let listener = TcpListener::bind(addr).await?;
    axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>())
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    tracing::info!("shutdown complete");
    Ok(())
}

/// Wait for SIGINT or SIGTERM, then return so axum's graceful shutdown
/// drains in-flight requests. On non-Unix platforms, only Ctrl-C is
/// honored (tokio does not expose SIGTERM elsewhere).
async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl-C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => tracing::info!("shutdown: SIGINT received"),
        _ = terminate => tracing::info!("shutdown: SIGTERM received"),
    }
}
