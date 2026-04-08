use std::future::Future;
use std::pin::Pin;

use anyhow::Context;
use serde::Deserialize;

use super::{IpRangeProvider, ProviderRecord};

const ORACLE_URL: &str = "https://docs.oracle.com/en-us/iaas/tools/public_ip_ranges.json";

#[derive(Debug, Deserialize)]
struct OracleResponse {
    regions: Vec<OracleRegion>,
}

#[derive(Debug, Deserialize)]
struct OracleRegion {
    region: String,
    cidrs: Vec<OracleCidr>,
}

#[derive(Debug, Deserialize)]
struct OracleCidr {
    cidr: String,
    tags: Vec<String>,
}

pub struct OracleProvider;

impl IpRangeProvider for OracleProvider {
    fn name(&self) -> &'static str {
        "oracle"
    }

    fn fetch<'a>(
        &'a self,
        client: &'a reqwest::Client,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<Vec<ProviderRecord>>> + Send + 'a>> {
        Box::pin(self.fetch_inner(client))
    }
}

impl OracleProvider {
    async fn fetch_inner(&self, client: &reqwest::Client) -> anyhow::Result<Vec<ProviderRecord>> {
        let resp: OracleResponse = client
            .get(ORACLE_URL)
            .send()
            .await
            .context("failed to fetch Oracle IP ranges")?
            .json()
            .await
            .context("failed to parse Oracle IP ranges")?;

        let mut records = Vec::new();

        for region in resp.regions {
            for cidr_entry in region.cidrs {
                records.push(ProviderRecord {
                    provider: "oracle".to_string(),
                    cidr: cidr_entry.cidr,
                    region: Some(region.region.clone()),
                    service: cidr_entry.tags.first().cloned(),
                });
            }
        }

        Ok(records)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_oracle_response() {
        let json = r#"{
            "last_updated_timestamp": "2024-01-01T00:00:00.000000",
            "regions": [
                {
                    "region": "us-ashburn-1",
                    "cidrs": [
                        {
                            "cidr": "129.146.0.0/21",
                            "tags": ["OCI"]
                        },
                        {
                            "cidr": "129.146.8.0/22",
                            "tags": ["OSN", "OBJECT_STORAGE"]
                        }
                    ]
                },
                {
                    "region": "eu-frankfurt-1",
                    "cidrs": [
                        {
                            "cidr": "138.1.0.0/20",
                            "tags": ["OCI"]
                        }
                    ]
                }
            ]
        }"#;

        let resp: OracleResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.regions.len(), 2);
        assert_eq!(resp.regions[0].region, "us-ashburn-1");
        assert_eq!(resp.regions[0].cidrs.len(), 2);
        assert_eq!(resp.regions[0].cidrs[0].cidr, "129.146.0.0/21");
        assert_eq!(resp.regions[0].cidrs[0].tags, vec!["OCI"]);
        assert_eq!(resp.regions[1].cidrs.len(), 1);
    }

    #[test]
    fn test_flatten_oracle_records() {
        let json = r#"{
            "last_updated_timestamp": "2024-01-01",
            "regions": [
                {
                    "region": "us-ashburn-1",
                    "cidrs": [
                        {"cidr": "10.0.0.0/8", "tags": ["OCI"]},
                        {"cidr": "172.16.0.0/12", "tags": ["OSN"]}
                    ]
                }
            ]
        }"#;

        let resp: OracleResponse = serde_json::from_str(json).unwrap();
        let mut records = Vec::new();
        for region in resp.regions {
            for cidr_entry in region.cidrs {
                records.push(ProviderRecord {
                    provider: "oracle".to_string(),
                    cidr: cidr_entry.cidr,
                    region: Some(region.region.clone()),
                    service: cidr_entry.tags.first().cloned(),
                });
            }
        }

        assert_eq!(records.len(), 2);
        assert_eq!(records[0].cidr, "10.0.0.0/8");
        assert_eq!(records[0].region.as_deref(), Some("us-ashburn-1"));
        assert_eq!(records[0].service.as_deref(), Some("OCI"));
        assert_eq!(records[1].cidr, "172.16.0.0/12");
        assert_eq!(records[1].service.as_deref(), Some("OSN"));
    }
}
