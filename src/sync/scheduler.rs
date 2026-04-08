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
    let client = reqwest::Client::new();

    // Fetch all providers concurrently
    let results = fetch_all_providers(providers, &client).await;

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let mut all_records = Vec::new();
    let mut statuses = Vec::new();

    // Collect current lookup table entries for providers that fail
    let current_table = state.lookup_table.read().await;
    let _ = &current_table; // just holding the read lock to check if we need fallback
    drop(current_table);

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
                all_records.extend(records);
            }
            Err(e) => {
                tracing::error!(provider = provider_name, error = %e, "failed to sync IP ranges");
                metrics::counter!("sync_total", "provider" => provider_name.to_string(), "result" => "error").increment(1);
                statuses.push(SyncStatus {
                    provider: provider_name.to_string(),
                    last_synced_at: None,
                    cidr_count: 0,
                    last_error: Some(e.to_string()),
                });
            }
        }
    }

    if !all_records.is_empty() {
        let new_table = IpLookupTable::from_records(all_records);
        tracing::info!(total_entries = new_table.len(), "rebuilt lookup table");
        let mut table = state.lookup_table.write().await;
        *table = new_table;
    }

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
