use std::fmt;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Config {
    port: u16,
}
impl Config {
    pub fn port(&self) -> u16 {
        self.port
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    TooManyLayers,
    Port,
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::TooManyLayers => "at most eight layers",
            Self::Port => "effective port must be in 1..=65535",
        })
    }
}
impl std::error::Error for Error {}
pub fn resolve(layers: &[Option<&str>]) -> Result<Config, Error> {
    if layers.len() > 8 {
        return Err(Error::TooManyLayers);
    }
    let mut effective = "8080";
    for value in layers.iter().flatten() {
        effective = value;
    }
    let port = effective.parse::<u16>().map_err(|_| Error::Port)?;
    if port == 0 {
        return Err(Error::Port);
    }
    Ok(Config { port })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn defaults_and_later_override() {
        assert_eq!(resolve(&[]).unwrap().port(), 8080);
        assert_eq!(
            resolve(&[Some("bad"), None, Some("9000")]).unwrap().port(),
            9000
        );
    }
    #[test]
    fn effective_invalid_is_rejected() {
        for value in ["0", "65536", "bad", ""] {
            assert_eq!(resolve(&[Some("9000"), Some(value)]), Err(Error::Port));
        }
    }
    #[test]
    fn exact_numeric_and_layer_boundaries() {
        assert_eq!(resolve(&[Some("1")]).unwrap().port(), 1);
        assert_eq!(resolve(&[Some("65535")]).unwrap().port(), 65535);
        assert!(resolve(&[None; 8]).is_ok());
        assert_eq!(resolve(&[None; 9]), Err(Error::TooManyLayers));
    }
}
