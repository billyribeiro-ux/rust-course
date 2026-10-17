use std::fmt;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    Number,
    Overflow,
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Number => "expected a signed decimal i64 with at most 20 bytes",
            Self::Overflow => "sum is outside the i64 range",
        })
    }
}
impl std::error::Error for Error {}
fn number(text: &str) -> Result<i64, Error> {
    if text.is_empty() || text.len() > 20 {
        return Err(Error::Number);
    }
    text.parse().map_err(|_| Error::Number)
}
pub fn add_text(left: &str, right: &str) -> Result<i64, Error> {
    number(left)?
        .checked_add(number(right)?)
        .ok_or(Error::Overflow)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn normal_and_negative_sums() {
        assert_eq!(add_text("7", "5"), Ok(12));
        assert_eq!(add_text("-7", "2"), Ok(-5));
    }
    #[test]
    fn overflow_is_distinct_from_parsing() {
        assert_eq!(add_text("9223372036854775807", "1"), Err(Error::Overflow));
        assert_eq!(add_text("9223372036854775808", "0"), Err(Error::Number));
        assert_eq!(add_text("3m", "4"), Err(Error::Number));
        assert_eq!(add_text(" 3", "4"), Err(Error::Number));
    }
    #[test]
    fn minimum_and_zero_are_valid() {
        assert_eq!(add_text("-9223372036854775808", "0"), Ok(i64::MIN));
    }
}
