//! Resolve textual layers first, then validate into a typed configuration.
//! ```
//! use typed_config::{Config, Layer};
//! let file = Layer::parse("port=9000\ntimeout_seconds=20")?;
//! let override_layer = Layer::parse("port=9100")?;
//! let config = Config::resolve([file, override_layer])?;
//! assert_eq!(config.port().get(), 9100);
//! # Ok::<(), typed_config::ConfigError>(())
//! ```
use std::{collections::BTreeMap, fmt, net::IpAddr, num::NonZeroU16, time::Duration};
const KEYS: [&str; 5] = [
    "host",
    "port",
    "timeout_seconds",
    "max_connections",
    "token",
];
#[derive(Clone, Default)]
pub struct Layer {
    values: BTreeMap<String, String>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigError {
    pub key: String,
    pub problem: &'static str,
}
impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "configuration key {}: {}", self.key, self.problem)
    }
}
impl std::error::Error for ConfigError {}
fn error(key: &str, problem: &'static str) -> ConfigError {
    ConfigError {
        key: key.into(),
        problem,
    }
}
// Debug intentionally redacts every value: even an unknown key might carry a secret.
impl fmt::Debug for Layer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Layer")
            .field("keys", &self.values.keys().collect::<Vec<_>>())
            .finish()
    }
}
impl Layer {
    /// Strict key=value lines, ASCII key names, full-line # comments only.
    /// Values are trimmed. Duplicate or unknown keys fail within a layer.
    pub fn parse(text: &str) -> Result<Self, ConfigError> {
        if text.len() > 16_384 {
            return Err(error("file", "configuration exceeds 16 KiB"));
        }
        let mut layer = Self::default();
        for line in text
            .lines()
            .map(str::trim)
            .filter(|s| !s.is_empty() && !s.starts_with('#'))
        {
            let (key, value) = line
                .split_once('=')
                .ok_or_else(|| error("file", "expected key=value"))?;
            layer.insert(key.trim(), value.trim())?;
        }
        Ok(layer)
    }
    pub fn insert(&mut self, key: &str, value: &str) -> Result<(), ConfigError> {
        if !KEYS.contains(&key) {
            return Err(error("unknown", "unknown configuration key"));
        }
        if self.values.contains_key(key) {
            return Err(error(key, "duplicate within one layer"));
        }
        if value.len() > 4096 {
            return Err(error(key, "value exceeds 4096 bytes"));
        }
        self.values.insert(key.into(), value.into());
        Ok(())
    }
    /// Accepts an injected iterator so tests never mutate process environment.
    pub fn environment<I, K, V>(pairs: I) -> Result<Self, ConfigError>
    where
        I: IntoIterator<Item = (K, V)>,
        K: AsRef<str>,
        V: AsRef<str>,
    {
        let mut layer = Self::default();
        for (name, value) in pairs {
            let key = match name.as_ref() {
                "APP_HOST" => "host",
                "APP_PORT" => "port",
                "APP_TIMEOUT_SECONDS" => "timeout_seconds",
                "APP_MAX_CONNECTIONS" => "max_connections",
                "APP_TOKEN" => "token",
                _ => continue,
            };
            layer.insert(key, value.as_ref())?;
        }
        Ok(layer)
    }
}
#[derive(Clone)]
pub struct Secret(String);
impl Secret {
    pub fn expose(&self) -> &str {
        &self.0
    }
}
impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[REDACTED]")
    }
}
#[derive(Clone, Debug)]
pub struct Config {
    host: IpAddr,
    port: NonZeroU16,
    timeout: Duration,
    max_connections: u16,
    token: Option<Secret>,
}
impl Config {
    pub fn host(&self) -> IpAddr {
        self.host
    }
    pub fn port(&self) -> NonZeroU16 {
        self.port
    }
    pub fn timeout(&self) -> Duration {
        self.timeout
    }
    pub fn max_connections(&self) -> u16 {
        self.max_connections
    }
    pub fn token(&self) -> Option<&Secret> {
        self.token.as_ref()
    }
    /// Later layers win. Only final effective values undergo semantic validation.
    pub fn resolve(layers: impl IntoIterator<Item = Layer>) -> Result<Self, ConfigError> {
        let mut raw: BTreeMap<String, String> = [
            ("host", "127.0.0.1"),
            ("port", "8080"),
            ("timeout_seconds", "30"),
            ("max_connections", "100"),
        ]
        .into_iter()
        .map(|(k, v)| (k.into(), v.into()))
        .collect();
        for layer in layers {
            raw.extend(layer.values);
        }
        let host = raw["host"]
            .parse::<IpAddr>()
            .map_err(|_| error("host", "expected an IPv4 or IPv6 address"))?;
        let port = raw["port"]
            .parse::<NonZeroU16>()
            .map_err(|_| error("port", "expected an integer in 1..=65535"))?;
        let seconds = raw["timeout_seconds"]
            .parse::<u64>()
            .map_err(|_| error("timeout_seconds", "expected an integer in 1..=300"))?;
        if !(1..=300).contains(&seconds) {
            return Err(error("timeout_seconds", "expected an integer in 1..=300"));
        }
        let max_connections = raw["max_connections"]
            .parse::<u16>()
            .map_err(|_| error("max_connections", "expected an integer in 1..=10000"))?;
        if !(1..=10_000).contains(&max_connections) {
            return Err(error("max_connections", "expected an integer in 1..=10000"));
        }
        let token = match raw.remove("token") {
            None => None,
            Some(token) => {
                if token.len() < 12
                    || token.chars().any(char::is_whitespace)
                    || token.chars().any(char::is_control)
                {
                    return Err(error(
                        "token",
                        "expected at least 12 bytes with no whitespace or controls",
                    ));
                }
                Some(Secret(token))
            }
        };
        Ok(Self {
            host,
            port,
            timeout: Duration::from_secs(seconds),
            max_connections,
            token,
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn later_layers_win_before_validation() {
        let file = Layer::parse("port=not-yet-valid\ntimeout_seconds=10").unwrap();
        let env = Layer::environment([("APP_PORT", "9001"), ("UNRELATED", "ignored")]).unwrap();
        let cli = Layer::parse("port=9002").unwrap();
        let c = Config::resolve([file, env, cli]).unwrap();
        assert_eq!(c.port().get(), 9002);
        assert_eq!(c.timeout(), Duration::from_secs(10));
    }
    #[test]
    fn errors_and_debug_do_not_include_secret_values() {
        let marker = "super-secret-token";
        let layer = Layer::parse(&format!("token={marker}")).unwrap();
        assert!(!format!("{layer:?}").contains(marker));
        let c = Config::resolve([layer]).unwrap();
        assert!(!format!("{c:?}").contains(marker));
        assert_eq!(c.token().unwrap().expose(), marker);
        let invalid = Layer::parse("token=bad secret").unwrap();
        let e = Config::resolve([invalid]).unwrap_err();
        assert!(!e.to_string().contains("bad secret"));
        assert!(Layer::parse("token=x\ntoken=y").is_err());
        assert!(Layer::parse("typo=x").is_err());
    }
    #[test]
    fn boundaries() {
        for port in ["0", "65536", "-1", "abc"] {
            assert!(Config::resolve([Layer::parse(&format!("port={port}")).unwrap()]).is_err());
        }
        for seconds in [1, 300] {
            assert!(
                Config::resolve([Layer::parse(&format!("timeout_seconds={seconds}")).unwrap()])
                    .is_ok()
            );
        }
        for seconds in [0, 301] {
            assert!(
                Config::resolve([Layer::parse(&format!("timeout_seconds={seconds}")).unwrap()])
                    .is_err()
            );
        }
    }
}
