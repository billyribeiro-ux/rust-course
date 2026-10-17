//! Learner task: merge textual layers, validate final port, and redact secrets.
use std::collections::BTreeMap;
#[derive(Default)]
pub struct Layer(pub BTreeMap<String, String>);
#[derive(Debug, PartialEq, Eq)]
pub struct ConfigError {
    pub key: &'static str,
    pub problem: &'static str,
}
pub struct Config {
    port: u16,
    token: Option<String>,
}
impl Config {
    pub fn resolve(layers: impl IntoIterator<Item = Layer>) -> Result<Self, ConfigError> {
        let _ = layers;
        todo!(
            "later wins; default 8080; port 1..=65535; errors name key and rule without echoing input"
        )
    }
    pub fn port(&self) -> u16 {
        self.port
    }
}
impl std::fmt::Debug for Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let _ = f;
        todo!("render port and redact token")
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn layer(k: &str, v: &str) -> Layer {
        Layer([(k.into(), v.into())].into())
    }
    #[test]
    fn precedence() {
        assert_eq!(
            Config::resolve([layer("port", "bad"), layer("port", "9000")])
                .unwrap()
                .port(),
            9000
        );
    }
    #[test]
    fn defaults() {
        assert_eq!(Config::resolve([]).unwrap().port(), 8080);
    }
    #[test]
    fn invalid() {
        assert!(Config::resolve([layer("port", "0")]).is_err());
    }
    #[test]
    fn redacts() {
        let c = Config::resolve([layer("token", "private-token-value")]).unwrap();
        assert!(!format!("{c:?}").contains("private-token-value"));
    }
}
