use std::sync::Arc;

use tokio::sync::RwLock;

use crate::config::Config;
use crate::lookup::IpLookupTable;

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
    pub config: Arc<Config>,
}

impl AppState {
    pub fn new(config: Config) -> Self {
        Self {
            lookup_table: Arc::new(RwLock::new(IpLookupTable::empty())),
            sync_status: Arc::new(RwLock::new(Vec::new())),
            config: Arc::new(config),
        }
    }
}
