pub mod aws;
pub mod gcp;
pub mod oracle;

use std::future::Future;
use std::pin::Pin;

#[derive(Debug, Clone)]
pub struct ProviderRecord {
    pub provider: String,
    pub cidr: String,
    pub region: Option<String>,
    pub service: Option<String>,
}

pub trait IpRangeProvider: Send + Sync {
    fn name(&self) -> &'static str;
    fn fetch<'a>(
        &'a self,
        client: &'a reqwest::Client,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<Vec<ProviderRecord>>> + Send + 'a>>;
}
