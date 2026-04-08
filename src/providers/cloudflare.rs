use std::future::Future;
use std::pin::Pin;

use anyhow::Context;
use serde::Deserialize;

use super::{IpRangeProvider, ProviderRecord};

const CLOUDFLARE_URL: &str = "https://api.cloudflare.com/client/v4/ips";

#[derive(Debug, Deserialize)]
struct CloudflareResponse {
    result: CloudflareResult,
    success: bool,
}

#[derive(Debug, Deserialize)]
struct CloudflareResult {
    ipv4_cidrs: Vec<String>,
    ipv6_cidrs: Vec<String>,
}

pub struct CloudflareProvider;

impl IpRangeProvider for CloudflareProvider {
    fn name(&self) -> &'static str {
        "cloudflare"
    }

    fn fetch<'a>(
        &'a self,
        client: &'a reqwest::Client,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<Vec<ProviderRecord>>> + Send + 'a>> {
        Box::pin(self.fetch_inner(client))
    }
}

impl CloudflareProvider {
    async fn fetch_inner(&self, client: &reqwest::Client) -> anyhow::Result<Vec<ProviderRecord>> {
        let resp: CloudflareResponse = client
            .get(CLOUDFLARE_URL)
            .send()
            .await
            .context("failed to fetch Cloudflare IP ranges")?
            .json()
            .await
            .context("failed to parse Cloudflare IP ranges")?;

        if !resp.success {
            anyhow::bail!("Cloudflare API returned success=false");
        }

        let mut records =
            Vec::with_capacity(resp.result.ipv4_cidrs.len() + resp.result.ipv6_cidrs.len());

        for cidr in resp.result.ipv4_cidrs {
            records.push(ProviderRecord {
                provider: "cloudflare".to_string(),
                cidr,
                region: None,
                service: Some("CDN".to_string()),
            });
        }

        for cidr in resp.result.ipv6_cidrs {
            records.push(ProviderRecord {
                provider: "cloudflare".to_string(),
                cidr,
                region: None,
                service: Some("CDN".to_string()),
            });
        }

        Ok(records)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_cloudflare_response() {
        let json = r#"{
            "result": {
                "ipv4_cidrs": [
                    "173.245.48.0/20",
                    "103.21.244.0/22",
                    "103.22.200.0/22"
                ],
                "ipv6_cidrs": [
                    "2400:cb00::/32",
                    "2606:4700::/32"
                ],
                "etag": "abc123"
            },
            "success": true,
            "errors": [],
            "messages": []
        }"#;

        let resp: CloudflareResponse = serde_json::from_str(json).unwrap();
        assert!(resp.success);
        assert_eq!(resp.result.ipv4_cidrs.len(), 3);
        assert_eq!(resp.result.ipv6_cidrs.len(), 2);
        assert_eq!(resp.result.ipv4_cidrs[0], "173.245.48.0/20");
        assert_eq!(resp.result.ipv6_cidrs[0], "2400:cb00::/32");
    }

    #[test]
    fn test_parse_cloudflare_failure_response() {
        let json = r#"{
            "result": {
                "ipv4_cidrs": [],
                "ipv6_cidrs": [],
                "etag": ""
            },
            "success": false,
            "errors": [{"message": "something went wrong"}],
            "messages": []
        }"#;

        let resp: CloudflareResponse = serde_json::from_str(json).unwrap();
        assert!(!resp.success);
    }
}
