use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tokio::time::MissedTickBehavior;

use crate::lookup::IpLookupTable;
use crate::providers::aws::AwsProvider;
use crate::providers::cloudflare::CloudflareProvider;
use crate::providers::gcp::GcpProvider;
use crate::providers::oracle::OracleProvider;
use crate::providers::{IpRangeProvider, ProviderRecord};
use crate::state::{AppState, SyncStatus};

pub async fn start_sync_loop(state: AppState) {
    let providers: Vec<Box<dyn IpRangeProvider>> = vec![
        Box::new(AwsProvider),
        Box::new(CloudflareProvider),
        Box::new(GcpProvider),
        Box::new(OracleProvider),
    ];

    // Run immediately on startup
    run_sync(&providers, &state).await;

    let mut interval =
        tokio::time::interval(Duration::from_secs(state.config.sync_interval_secs));
    interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

    loop {
        interval.tick().await;
        run_sync(&providers, &state).await;
    }
}

async fn run_sync(providers: &[Box<dyn IpRangeProvider>], state: &AppState) {
    tracing::info!("starting IP range sync");
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(30))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new());

    // Fetch all providers concurrently
    let results = fetch_all_providers(providers, &client).await;

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let mut statuses = Vec::new();
    let mut provider_records = state.provider_records.write().await;

    for (provider_name, result) in results {
        match result {
            Ok(records) => {
                let count = records.len();
                tracing::info!(provider = provider_name, count, "synced IP ranges");
                metrics::counter!("sync_total", "provider" => provider_name.to_string(), "result" => "success").increment(1);
                metrics::gauge!("sync_cidr_count", "provider" => provider_name.to_string()).set(count as f64);
                statuses.push(SyncStatus {
                    provider: provider_name.to_string(),
                    last_synced_at: Some(now),
                    cidr_count: count,
                    last_error: None,
                });
                provider_records.insert(provider_name.to_string(), records);
            }
            Err(e) => {
                let retained = provider_records
                    .get(provider_name)
                    .map(|r| r.len())
                    .unwrap_or(0);
                tracing::error!(
                    provider = provider_name,
                    error = %e,
                    retained_cidrs = retained,
                    "failed to sync IP ranges; retaining last-known-good records"
                );
                metrics::counter!("sync_total", "provider" => provider_name.to_string(), "result" => "error").increment(1);
                statuses.push(SyncStatus {
                    provider: provider_name.to_string(),
                    last_synced_at: None,
                    cidr_count: retained,
                    last_error: Some(e.to_string()),
                });
            }
        }
    }

    let all_records: Vec<ProviderRecord> = provider_records
        .values()
        .flat_map(|v| v.iter().cloned())
        .collect();

    if !all_records.is_empty() {
        let new_table = IpLookupTable::from_records(all_records);
        tracing::info!(total_entries = new_table.len(), "rebuilt lookup table");
        let mut table = state.lookup_table.write().await;
        *table = new_table;
    }
    drop(provider_records);

    let mut sync_status = state.sync_status.write().await;
    *sync_status = statuses;

    tracing::info!("IP range sync complete");
}

async fn fetch_all_providers(
    providers: &[Box<dyn IpRangeProvider>],
    client: &reqwest::Client,
) -> Vec<(&'static str, anyhow::Result<Vec<ProviderRecord>>)> {
    let futures: Vec<_> = providers
        .iter()
        .map(|p| {
            let client = client.clone();
            let name = p.name();
            async move { (name, p.fetch(&client).await) }
        })
        .collect();

    futures::future::join_all(futures).await
}

#[cfg(test)]
mod tests {
    use std::future::Future;
    use std::pin::Pin;
    use std::sync::Mutex;

    use metrics_exporter_prometheus::PrometheusBuilder;

    use super::*;
    use crate::config::Config;

    /// Controllable stub provider for testing retention behavior.
    struct StubProvider {
        name: &'static str,
        results: Mutex<Vec<anyhow::Result<Vec<ProviderRecord>>>>,
    }

    impl StubProvider {
        fn new(name: &'static str, results: Vec<anyhow::Result<Vec<ProviderRecord>>>) -> Self {
            Self {
                name,
                results: Mutex::new(results),
            }
        }
    }

    impl IpRangeProvider for StubProvider {
        fn name(&self) -> &'static str {
            self.name
        }
        fn fetch<'a>(
            &'a self,
            _client: &'a reqwest::Client,
        ) -> Pin<Box<dyn Future<Output = anyhow::Result<Vec<ProviderRecord>>> + Send + 'a>> {
            let next = self.results.lock().unwrap().remove(0);
            Box::pin(async move { next })
        }
    }

    fn test_config() -> Config {
        Config {
            port: 0,
            sync_interval_secs: 43200,
            log_level: "info".to_string(),
            trusted_proxies: vec![],
            rate_limit_per_second: 10,
            rate_limit_burst: 10,
            excluded_headers: vec![],
        }
    }

    fn rec(provider: &str, cidr: &str) -> ProviderRecord {
        ProviderRecord {
            provider: provider.to_string(),
            cidr: cidr.to_string(),
            region: None,
            service: None,
        }
    }

    #[tokio::test]
    async fn partial_failure_retains_last_known_good() {
        // Install a throwaway Prometheus recorder so metrics::counter! calls
        // have somewhere to go even if another test already installed one.
        let _ = PrometheusBuilder::new().install_recorder();

        let state = AppState::new(test_config(), PrometheusBuilder::new().build_recorder().handle());

        let providers: Vec<Box<dyn IpRangeProvider>> = vec![
            Box::new(StubProvider::new(
                "alpha",
                vec![
                    Ok(vec![rec("alpha", "10.0.0.0/8")]),
                    Err(anyhow::anyhow!("boom")),
                ],
            )),
            Box::new(StubProvider::new(
                "beta",
                vec![
                    Ok(vec![rec("beta", "20.0.0.0/8")]),
                    Ok(vec![rec("beta", "20.0.0.0/8"), rec("beta", "21.0.0.0/8")]),
                ],
            )),
        ];

        // First run: both succeed.
        run_sync(&providers, &state).await;

        let table_len_after_first = state.lookup_table.read().await.len();
        assert_eq!(table_len_after_first, 2);

        // Second run: alpha fails, beta succeeds with a new record.
        run_sync(&providers, &state).await;

        let table = state.lookup_table.read().await;
        // Expect alpha's 10/8 retained + beta's two new records = 3 total.
        assert_eq!(table.len(), 3);
        assert!(
            table.lookup("10.0.0.1".parse().unwrap()).is_some(),
            "alpha's last-known-good record should still be present after its fetch failed"
        );
        assert!(table.lookup("21.0.0.1".parse().unwrap()).is_some());

        let status = state.sync_status.read().await;
        let alpha_status = status.iter().find(|s| s.provider == "alpha").unwrap();
        assert!(alpha_status.last_error.is_some());
        assert_eq!(alpha_status.cidr_count, 1, "retained count reported in status");
    }
}
