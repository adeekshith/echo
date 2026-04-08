use std::future::Future;
use std::pin::Pin;

use anyhow::Context;
use serde::Deserialize;

use super::{IpRangeProvider, ProviderRecord};

const AWS_URL: &str = "https://ip-ranges.amazonaws.com/ip-ranges.json";

#[derive(Debug, Deserialize)]
struct AwsResponse {
    prefixes: Vec<AwsPrefix>,
    #[serde(default)]
    ipv6_prefixes: Vec<AwsIpv6Prefix>,
}

#[derive(Debug, Deserialize)]
struct AwsPrefix {
    ip_prefix: String,
    region: String,
    service: String,
}

#[derive(Debug, Deserialize)]
struct AwsIpv6Prefix {
    ipv6_prefix: String,
    region: String,
    service: String,
}

pub struct AwsProvider;

impl IpRangeProvider for AwsProvider {
    fn name(&self) -> &'static str {
        "aws"
    }

    fn fetch<'a>(
        &'a self,
        client: &'a reqwest::Client,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<Vec<ProviderRecord>>> + Send + 'a>> {
        Box::pin(self.fetch_inner(client))
    }
}

impl AwsProvider {
    async fn fetch_inner(&self, client: &reqwest::Client) -> anyhow::Result<Vec<ProviderRecord>> {
        let resp: AwsResponse = client
            .get(AWS_URL)
            .send()
            .await
            .context("failed to fetch AWS IP ranges")?
            .json()
            .await
            .context("failed to parse AWS IP ranges")?;

        let mut records = Vec::with_capacity(resp.prefixes.len() + resp.ipv6_prefixes.len());

        for p in resp.prefixes {
            records.push(ProviderRecord {
                provider: "aws".to_string(),
                cidr: p.ip_prefix,
                region: Some(p.region),
                service: Some(p.service),
            });
        }

        for p in resp.ipv6_prefixes {
            records.push(ProviderRecord {
                provider: "aws".to_string(),
                cidr: p.ipv6_prefix,
                region: Some(p.region),
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
    fn test_parse_aws_response() {
        let json = r#"{
            "syncToken": "123",
            "createDate": "2024-01-01-00-00-00",
            "prefixes": [
                {
                    "ip_prefix": "3.4.12.4/32",
                    "region": "eu-west-1",
                    "service": "AMAZON",
                    "network_border_group": "eu-west-1"
                }
            ],
            "ipv6_prefixes": [
                {
                    "ipv6_prefix": "2600:1f00::/24",
                    "region": "us-east-1",
                    "service": "AMAZON",
                    "network_border_group": "us-east-1"
                }
            ]
        }"#;

        let resp: AwsResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.prefixes.len(), 1);
        assert_eq!(resp.prefixes[0].ip_prefix, "3.4.12.4/32");
        assert_eq!(resp.prefixes[0].region, "eu-west-1");
        assert_eq!(resp.prefixes[0].service, "AMAZON");
        assert_eq!(resp.ipv6_prefixes.len(), 1);
        assert_eq!(resp.ipv6_prefixes[0].ipv6_prefix, "2600:1f00::/24");
    }
}
