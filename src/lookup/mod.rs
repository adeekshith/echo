use std::net::IpAddr;

use ipnet::IpNet;

use crate::providers::ProviderRecord;

#[derive(Debug, Clone)]
pub struct LookupEntry {
    pub network: IpNet,
    pub provider: String,
    pub region: Option<String>,
    pub service: Option<String>,
}

#[derive(Debug)]
pub struct IpLookupTable {
    entries: Vec<LookupEntry>,
}

impl IpLookupTable {
    pub fn empty() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    pub fn from_records(records: Vec<ProviderRecord>) -> Self {
        let mut entries: Vec<LookupEntry> = records
            .into_iter()
            .filter_map(|r| {
                let network: IpNet = r.cidr.parse().ok()?;
                Some(LookupEntry {
                    network,
                    provider: r.provider,
                    region: r.region,
                    service: r.service,
                })
            })
            .collect();

        // Sort by prefix length descending for longest-prefix match
        entries.sort_by(|a, b| b.network.prefix_len().cmp(&a.network.prefix_len()));

        Self { entries }
    }

    pub fn lookup(&self, ip: IpAddr) -> Option<&LookupEntry> {
        let normalized = normalize_ip(ip);
        self.entries.iter().find(|e| e.network.contains(&normalized))
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Normalize IPv4-mapped IPv6 addresses (::ffff:x.x.x.x) to plain IPv4.
fn normalize_ip(ip: IpAddr) -> IpAddr {
    match ip {
        IpAddr::V6(v6) => match v6.to_ipv4_mapped() {
            Some(v4) => IpAddr::V4(v4),
            None => IpAddr::V6(v6),
        },
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_records() -> Vec<ProviderRecord> {
        vec![
            ProviderRecord {
                provider: "aws".to_string(),
                cidr: "10.0.0.0/8".to_string(),
                region: Some("us-east-1".to_string()),
                service: Some("AMAZON".to_string()),
            },
            ProviderRecord {
                provider: "aws".to_string(),
                cidr: "10.0.1.0/24".to_string(),
                region: Some("us-west-2".to_string()),
                service: Some("EC2".to_string()),
            },
            ProviderRecord {
                provider: "gcp".to_string(),
                cidr: "34.0.0.0/8".to_string(),
                region: Some("us-central1".to_string()),
                service: Some("Google Cloud".to_string()),
            },
            ProviderRecord {
                provider: "gcp".to_string(),
                cidr: "2600:1900::/28".to_string(),
                region: Some("us-central1".to_string()),
                service: Some("Google Cloud".to_string()),
            },
        ]
    }

    #[test]
    fn test_longest_prefix_match() {
        let table = IpLookupTable::from_records(make_records());

        // 10.0.1.50 matches both 10.0.0.0/8 and 10.0.1.0/24
        // Should return the longer prefix (10.0.1.0/24)
        let result = table.lookup("10.0.1.50".parse().unwrap()).unwrap();
        assert_eq!(result.provider, "aws");
        assert_eq!(result.region.as_deref(), Some("us-west-2"));
        assert_eq!(result.service.as_deref(), Some("EC2"));
    }

    #[test]
    fn test_shorter_prefix_match() {
        let table = IpLookupTable::from_records(make_records());

        // 10.0.2.1 matches 10.0.0.0/8 only
        let result = table.lookup("10.0.2.1".parse().unwrap()).unwrap();
        assert_eq!(result.region.as_deref(), Some("us-east-1"));
    }

    #[test]
    fn test_no_match() {
        let table = IpLookupTable::from_records(make_records());
        assert!(table.lookup("192.168.1.1".parse().unwrap()).is_none());
    }

    #[test]
    fn test_ipv6_lookup() {
        let table = IpLookupTable::from_records(make_records());

        let result = table
            .lookup("2600:1900:0001::1".parse().unwrap())
            .unwrap();
        assert_eq!(result.provider, "gcp");
    }

    #[test]
    fn test_ipv4_mapped_ipv6_normalization() {
        let table = IpLookupTable::from_records(make_records());

        // ::ffff:10.0.1.50 should be normalized to 10.0.1.50 and match
        let addr: IpAddr = "::ffff:10.0.1.50".parse().unwrap();
        let result = table.lookup(addr).unwrap();
        assert_eq!(result.provider, "aws");
        assert_eq!(result.region.as_deref(), Some("us-west-2"));
    }

    #[test]
    fn test_empty_table() {
        let table = IpLookupTable::empty();
        assert!(table.is_empty());
        assert!(table.lookup("10.0.0.1".parse().unwrap()).is_none());
    }

    #[test]
    fn test_invalid_cidr_skipped() {
        let records = vec![ProviderRecord {
            provider: "test".to_string(),
            cidr: "not-a-cidr".to_string(),
            region: None,
            service: None,
        }];
        let table = IpLookupTable::from_records(records);
        assert!(table.is_empty());
    }
}
