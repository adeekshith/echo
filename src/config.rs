use std::env;
use std::net::IpAddr;
use std::str::FromStr;

use ipnet::IpNet;

const DEFAULT_PORT: u16 = 8083;
const DEFAULT_SYNC_INTERVAL_SECS: u64 = 43200;
const DEFAULT_LOG_LEVEL: &str = "info";
const DEFAULT_TRUSTED_PROXIES: &str = "127.0.0.1/32,10.0.0.0/8,172.16.0.0/12,192.168.0.0/16";
const DEFAULT_RATE_LIMIT_PER_SECOND: u64 = 10;
const DEFAULT_RATE_LIMIT_BURST: u32 = 20;

#[derive(Debug, Clone)]
pub struct Config {
    pub port: u16,
    pub sync_interval_secs: u64,
    pub log_level: String,
    pub trusted_proxies: Vec<IpNet>,
    pub rate_limit_per_second: u64,
    pub rate_limit_burst: u32,
    pub excluded_headers: Vec<String>,
}

/// Read an env var. Returns `Ok(None)` if unset, `Ok(Some(raw))` if set
/// (even to an empty string), and `Err(...)` if set to invalid Unicode.
fn read_env(key: &str) -> Result<Option<String>, String> {
    match env::var(key) {
        Ok(v) => Ok(Some(v)),
        Err(env::VarError::NotPresent) => Ok(None),
        Err(env::VarError::NotUnicode(_)) => Err(format!("{key} is set to invalid Unicode")),
    }
}

/// Parse an env var that falls back to `default` only when unset. If the
/// env var is explicitly set, it must parse successfully and pass
/// `validate`, otherwise startup fails.
fn parse_env<T, F>(key: &str, default: T, validate: F) -> Result<T, String>
where
    T: FromStr,
    T::Err: std::fmt::Display,
    F: Fn(&T) -> Result<(), String>,
{
    match read_env(key)? {
        None => Ok(default),
        Some(raw) => {
            let parsed: T = raw
                .parse()
                .map_err(|e| format!("{key}=\"{raw}\" is not a valid value: {e}"))?;
            validate(&parsed).map_err(|e| format!("{key}=\"{raw}\" is invalid: {e}"))?;
            Ok(parsed)
        }
    }
}

fn parse_trusted_proxies(raw: &str) -> Result<Vec<IpNet>, String> {
    raw.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| {
            s.parse::<IpNet>()
                .map_err(|e| format!("\"{s}\" is not a valid CIDR: {e}"))
        })
        .collect()
}

impl Config {
    pub fn from_env() -> Result<Self, String> {
        let port = parse_env::<u16, _>("PORT", DEFAULT_PORT, |v| {
            if *v == 0 {
                Err("must be between 1 and 65535".into())
            } else {
                Ok(())
            }
        })?;

        let sync_interval_secs =
            parse_env::<u64, _>("SYNC_INTERVAL_SECS", DEFAULT_SYNC_INTERVAL_SECS, |v| {
                if *v == 0 {
                    Err("must be greater than 0".into())
                } else {
                    Ok(())
                }
            })?;

        let log_level = match read_env("LOG_LEVEL")? {
            Some(s) if !s.trim().is_empty() => s,
            _ => DEFAULT_LOG_LEVEL.to_string(),
        };

        let trusted_proxies = match read_env("TRUSTED_PROXIES")? {
            None => parse_trusted_proxies(DEFAULT_TRUSTED_PROXIES)
                .expect("built-in default TRUSTED_PROXIES should always parse"),
            Some(raw) => {
                let parsed = parse_trusted_proxies(&raw)
                    .map_err(|e| format!("TRUSTED_PROXIES is invalid: {e}"))?;
                if parsed.is_empty() {
                    return Err(format!(
                        "TRUSTED_PROXIES is set but contains no entries: \"{raw}\""
                    ));
                }
                parsed
            }
        };

        let rate_limit_per_second = parse_env::<u64, _>(
            "RATE_LIMIT_PER_SECOND",
            DEFAULT_RATE_LIMIT_PER_SECOND,
            |v| {
                if *v == 0 {
                    Err("must be greater than 0".into())
                } else {
                    Ok(())
                }
            },
        )?;

        let rate_limit_burst =
            parse_env::<u32, _>("RATE_LIMIT_BURST", DEFAULT_RATE_LIMIT_BURST, |v| {
                if *v == 0 {
                    Err("must be greater than 0".into())
                } else {
                    Ok(())
                }
            })?;

        let excluded_headers = match read_env("EXCLUDED_HEADERS")? {
            None => Vec::new(),
            Some(raw) => raw
                .split(',')
                .map(|s| s.trim().to_lowercase())
                .filter(|s| !s.is_empty())
                .collect(),
        };

        Ok(Self {
            port,
            sync_interval_secs,
            log_level,
            trusted_proxies,
            rate_limit_per_second,
            rate_limit_burst,
            excluded_headers,
        })
    }

    pub fn is_trusted_proxy(&self, ip: &IpAddr) -> bool {
        self.trusted_proxies.iter().any(|net| net.contains(ip))
    }

    pub fn is_header_excluded(&self, name: &str) -> bool {
        self.excluded_headers.iter().any(|h| h == name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // These tests mutate process-wide env vars, so they can't run in parallel
    // with each other. We serialize by defining a single #[test] that runs
    // each scenario sequentially; this keeps behavior simple without adding
    // a `serial_test` dep.
    fn clear_all() {
        unsafe {
            for k in [
                "PORT",
                "SYNC_INTERVAL_SECS",
                "LOG_LEVEL",
                "TRUSTED_PROXIES",
                "RATE_LIMIT_PER_SECOND",
                "RATE_LIMIT_BURST",
                "EXCLUDED_HEADERS",
            ] {
                env::remove_var(k);
            }
        }
    }

    #[test]
    fn env_driven_config_scenarios() {
        // Unset vars -> defaults.
        clear_all();
        let c = Config::from_env().expect("defaults should be valid");
        assert_eq!(c.port, DEFAULT_PORT);
        assert_eq!(c.sync_interval_secs, DEFAULT_SYNC_INTERVAL_SECS);
        assert_eq!(c.log_level, DEFAULT_LOG_LEVEL);
        assert_eq!(c.trusted_proxies.len(), 4);
        assert_eq!(c.rate_limit_per_second, DEFAULT_RATE_LIMIT_PER_SECOND);
        assert_eq!(c.rate_limit_burst, DEFAULT_RATE_LIMIT_BURST);
        assert!(c.excluded_headers.is_empty());

        // Explicit PORT=0 -> error.
        clear_all();
        unsafe { env::set_var("PORT", "0") };
        assert!(Config::from_env().is_err());

        // Explicit PORT=abc -> error (parse failure).
        clear_all();
        unsafe { env::set_var("PORT", "abc") };
        assert!(Config::from_env().is_err());

        // Explicit valid PORT.
        clear_all();
        unsafe { env::set_var("PORT", "9000") };
        assert_eq!(Config::from_env().unwrap().port, 9000);

        // SYNC_INTERVAL_SECS=0 -> error.
        clear_all();
        unsafe { env::set_var("SYNC_INTERVAL_SECS", "0") };
        assert!(Config::from_env().is_err());

        // RATE_LIMIT_PER_SECOND=0 -> error.
        clear_all();
        unsafe { env::set_var("RATE_LIMIT_PER_SECOND", "0") };
        assert!(Config::from_env().is_err());

        // RATE_LIMIT_BURST=0 -> error.
        clear_all();
        unsafe { env::set_var("RATE_LIMIT_BURST", "0") };
        assert!(Config::from_env().is_err());

        // TRUSTED_PROXIES set but empty -> error.
        clear_all();
        unsafe { env::set_var("TRUSTED_PROXIES", "   ") };
        assert!(Config::from_env().is_err());

        // TRUSTED_PROXIES with garbage entry -> error.
        clear_all();
        unsafe { env::set_var("TRUSTED_PROXIES", "not-a-cidr") };
        assert!(Config::from_env().is_err());

        // TRUSTED_PROXIES with one valid entry -> ok.
        clear_all();
        unsafe { env::set_var("TRUSTED_PROXIES", "10.0.0.0/8") };
        let c = Config::from_env().unwrap();
        assert_eq!(c.trusted_proxies.len(), 1);

        clear_all();
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
            excluded_headers: vec![],
        };

        let trusted: IpAddr = "10.0.0.1".parse().unwrap();
        let untrusted: IpAddr = "203.0.113.1".parse().unwrap();

        assert!(config.is_trusted_proxy(&trusted));
        assert!(!config.is_trusted_proxy(&untrusted));
    }
}
