use std::env;
use std::net::IpAddr;

use ipnet::IpNet;

#[derive(Debug, Clone)]
pub struct Config {
    pub port: u16,
    pub sync_interval_secs: u64,
    pub log_level: String,
    pub trusted_proxies: Vec<IpNet>,
    pub rate_limit_per_second: u64,
    pub rate_limit_burst: u32,
}

impl Config {
    pub fn from_env() -> Self {
        let port = env::var("PORT")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(8083);

        let sync_interval_secs = env::var("SYNC_INTERVAL_SECS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(43200);

        let log_level = env::var("LOG_LEVEL").unwrap_or_else(|_| "info".to_string());

        let trusted_proxies = env::var("TRUSTED_PROXIES")
            .unwrap_or_else(|_| {
                "127.0.0.1/32,10.0.0.0/8,172.16.0.0/12,192.168.0.0/16".to_string()
            })
            .split(',')
            .filter_map(|s| s.trim().parse::<IpNet>().ok())
            .collect();

        let rate_limit_per_second = env::var("RATE_LIMIT_PER_SECOND")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(10);

        let rate_limit_burst = env::var("RATE_LIMIT_BURST")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(20);

        Self {
            port,
            sync_interval_secs,
            log_level,
            trusted_proxies,
            rate_limit_per_second,
            rate_limit_burst,
        }
    }

    pub fn is_trusted_proxy(&self, ip: &IpAddr) -> bool {
        self.trusted_proxies.iter().any(|net| net.contains(ip))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        // Clear env vars to test defaults
        unsafe {
            env::remove_var("PORT");
            env::remove_var("SYNC_INTERVAL_SECS");
            env::remove_var("LOG_LEVEL");
            env::remove_var("TRUSTED_PROXIES");
            env::remove_var("RATE_LIMIT_PER_SECOND");
            env::remove_var("RATE_LIMIT_BURST");
        }

        let config = Config::from_env();
        assert_eq!(config.port, 8083);
        assert_eq!(config.sync_interval_secs, 43200);
        assert_eq!(config.log_level, "info");
        assert_eq!(config.trusted_proxies.len(), 4);
        assert_eq!(config.rate_limit_per_second, 10);
        assert_eq!(config.rate_limit_burst, 20);
    }

    #[test]
    fn test_trusted_proxy_check() {
        let config = Config {
            port: 8083,
            sync_interval_secs: 43200,
            log_level: "info".to_string(),
            trusted_proxies: vec!["10.0.0.0/8".parse().unwrap()],
            rate_limit_per_second: 10,
            rate_limit_burst: 20,
        };

        let trusted: IpAddr = "10.0.0.1".parse().unwrap();
        let untrusted: IpAddr = "203.0.113.1".parse().unwrap();

        assert!(config.is_trusted_proxy(&trusted));
        assert!(!config.is_trusted_proxy(&untrusted));
    }
}
