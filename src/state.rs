use std::collections::HashMap;
use std::sync::Arc;

use metrics_exporter_prometheus::PrometheusHandle;
use tokio::sync::RwLock;

use crate::config::Config;
use crate::lookup::IpLookupTable;
use crate::providers::ProviderRecord;

#[derive(Debug, Clone, serde::Serialize)]
pub struct SyncStatus {
    pub provider: String,
    pub last_synced_at: Option<u64>,
    pub cidr_count: usize,
    pub last_error: Option<String>,
}

#[derive(Clone)]
pub struct AppState {
    pub lookup_table: Arc<RwLock<IpLookupTable>>,
    pub sync_status: Arc<RwLock<Vec<SyncStatus>>>,
    /// Last-known-good records per provider name. On a fetch failure, the
    /// previous records are reused so partial sync failures don't drop data.
    pub provider_records: Arc<RwLock<HashMap<String, Vec<ProviderRecord>>>>,
    pub config: Arc<Config>,
    pub metrics_handle: PrometheusHandle,
}

impl AppState {
    pub fn new(config: Config, metrics_handle: PrometheusHandle) -> Self {
        Self {
            lookup_table: Arc::new(RwLock::new(IpLookupTable::empty())),
            sync_status: Arc::new(RwLock::new(Vec::new())),
            provider_records: Arc::new(RwLock::new(HashMap::new())),
            config: Arc::new(config),
            metrics_handle,
        }
    }
}
