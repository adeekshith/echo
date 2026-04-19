//! Shared helpers for integration tests.
//!
//! Extracted from per-file duplicates of `test_config`, `test_metrics_handle`,
//! and `test_state_with_table`. Suppress `dead_code` because not every test
//! file uses every helper.

#![allow(dead_code)]

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};
use tokio::sync::RwLock;

use axum::Router;

use ipecho::config::Config;
use ipecho::lookup::IpLookupTable;
use ipecho::ratelimit::RateLimitState;
use ipecho::routes::create_router;
use ipecho::state::AppState;

/// Permissive defaults suitable for most tests. Individual tests can mutate
/// the returned Config (e.g. shrinking rate limits to exercise rejection).
pub fn test_config() -> Config {
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

/// A throwaway PrometheusHandle for tests that don't inspect metrics output.
/// `build_recorder()` does *not* install globally, so `metrics::counter!`
/// calls from the app aren't captured — use `global_metrics_handle()` when
/// the test needs to read back emitted metrics.
pub fn throwaway_metrics_handle() -> PrometheusHandle {
    PrometheusBuilder::new().build_recorder().handle()
}

/// Install a single process-wide Prometheus recorder on first call and
/// return handles to the same registry thereafter. Needed by tests that
/// scrape `/metrics` and assert on counter values.
pub fn global_metrics_handle() -> PrometheusHandle {
    static HANDLE: OnceLock<PrometheusHandle> = OnceLock::new();
    HANDLE
        .get_or_init(|| {
            PrometheusBuilder::new()
                .install_recorder()
                .expect("failed to install global Prometheus recorder for tests")
        })
        .clone()
}

/// Default test state: permissive config, throwaway metrics, empty sync
/// status and provider records.
pub fn test_state_with_table(table: IpLookupTable) -> AppState {
    test_state(test_config(), throwaway_metrics_handle(), table)
}

/// Fully customizable state builder.
pub fn test_state(
    config: Config,
    metrics_handle: PrometheusHandle,
    table: IpLookupTable,
) -> AppState {
    AppState {
        lookup_table: Arc::new(RwLock::new(table)),
        sync_status: Arc::new(RwLock::new(vec![])),
        provider_records: Arc::new(RwLock::new(HashMap::new())),
        config: Arc::new(config),
        metrics_handle,
    }
}

/// Build an axum Router wired up with a fresh RateLimitState derived from
/// the state's config. Hides the two-argument router constructor from test
/// call sites that don't care about driving eviction.
pub fn build_router(state: AppState) -> Router {
    let rl_state = RateLimitState::new(
        state.config.rate_limit_per_second,
        state.config.rate_limit_burst,
    );
    create_router(state, rl_state)
}
