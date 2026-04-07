use std::future::Future;
use std::pin::Pin;

use anyhow::Context;
use serde::Deserialize;

use super::{IpRangeProvider, ProviderRecord};

const GCP_URL: &str = "https://www.gstatic.com/ipranges/cloud.json";

#[derive(Debug, Deserialize)]
struct GcpResponse {
    prefixes: Vec<GcpPrefix>,
}

#[derive(Debug, Deserialize)]
struct GcpPrefix {
    #[serde(rename = "ipv4Prefix")]
    ipv4_prefix: Option<String>,
    #[serde(rename = "ipv6Prefix")]
    ipv6_prefix: Option<String>,
    service: String,
    scope: String,
}

pub struct GcpProvider;

impl IpRangeProvider for GcpProvider {
    fn name(&self) -> &'static str {
        "gcp"
    }

    fn fetch<'a>(
        &'a self,
        client: &'a reqwest::Client,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<Vec<ProviderRecord>>> + Send + 'a>> {
        Box::pin(self.fetch_inner(client))
    }
}

impl GcpProvider {
    async fn fetch_inner(&self, client: &reqwest::Client) -> anyhow::Result<Vec<ProviderRecord>> {
        let resp: GcpResponse = client
            .get(GCP_URL)
            .send()
            .await
            .context("failed to fetch GCP IP ranges")?
            .json()
            .await
            .context("failed to parse GCP IP ranges")?;

        let mut records = Vec::with_capacity(resp.prefixes.len());

        for p in resp.prefixes {
            let cidr = match (p.ipv4_prefix, p.ipv6_prefix) {
                (Some(v4), _) => v4,
                (_, Some(v6)) => v6,
                _ => continue,
            };

            records.push(ProviderRecord {
                provider: "gcp".to_string(),
                cidr,
                region: Some(p.scope),
                service: Some(p.service),
            });
        }

        Ok(records)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_gcp_response() {
        let json = r#"{
            "syncToken": "123",
            "creationTime": "2024-01-01T00:00:00.00000",
            "prefixes": [
                {
                    "ipv4Prefix": "34.1.208.0/20",
                    "service": "Google Cloud",
                    "scope": "africa-south1"
                },
                {
                    "ipv6Prefix": "2600:1900:8000::/44",
                    "service": "Google Cloud",
                    "scope": "us-central1"
                }
            ]
        }"#;

        let resp: GcpResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.prefixes.len(), 2);
        assert_eq!(
            resp.prefixes[0].ipv4_prefix.as_deref(),
            Some("34.1.208.0/20")
        );
        assert!(resp.prefixes[0].ipv6_prefix.is_none());
        assert_eq!(resp.prefixes[1].ipv4_prefix, None);
        assert_eq!(
            resp.prefixes[1].ipv6_prefix.as_deref(),
            Some("2600:1900:8000::/44")
        );
    }
}
