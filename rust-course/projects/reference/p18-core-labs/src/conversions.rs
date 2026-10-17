use std::fmt;
use std::str::FromStr;

/// A whole percentage in 0..=100. Safe callers cannot bypass validation.
///
/// ```
/// use core_rust_labs::conversions::{Percent, PercentError};
/// let percent = Percent::try_from(25u16)?;
/// assert_eq!(u16::from(percent), 25);
/// # Ok::<(), PercentError>(())
/// ```
///
/// The tuple field remains private even though the type itself is public:
/// ```compile_fail
/// use core_rust_labs::conversions::Percent;
/// let invalid = Percent(255);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Percent(u8);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PercentError {
    Syntax,
    OutOfRange(u16),
}
impl fmt::Display for PercentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Syntax => f.write_str("expected 1 to 3 ASCII decimal digits"),
            Self::OutOfRange(value) => write!(f, "percentage {value} is outside 0..=100"),
        }
    }
}
impl std::error::Error for PercentError {}

impl TryFrom<u16> for Percent {
    type Error = PercentError;
    fn try_from(value: u16) -> Result<Self, Self::Error> {
        if value > 100 {
            return Err(PercentError::OutOfRange(value));
        }
        // The validated 0..=100 range is a subset of u8's range.
        Ok(Self(value as u8))
    }
}
impl From<Percent> for u16 {
    fn from(value: Percent) -> Self {
        u16::from(value.0)
    }
}
impl FromStr for Percent {
    type Err = PercentError;
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        if text.is_empty() || text.len() > 3 || !text.bytes().all(|b| b.is_ascii_digit()) {
            return Err(PercentError::Syntax);
        }
        let value = text.parse::<u16>().map_err(|_| PercentError::Syntax)?;
        Self::try_from(value)
    }
}
impl fmt::Display for Percent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Parse the whole batch or return its first error; no partial result is published.
pub fn parse_percentages(texts: &[&str]) -> Result<Vec<Percent>, PercentError> {
    texts.iter().map(|text| text.parse()).collect()
}

/// Keep input order and return basis points for values at or above the threshold.
/// Result length is at most input length; no sorting or deduplication is implied.
pub fn selected_basis_points(values: &[Percent], threshold: Percent) -> Vec<u16> {
    values
        .iter()
        .copied()
        .filter(|value| *value >= threshold)
        .map(|value| u16::from(value) * 100) // At most 10,000, which fits u16.
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conversions_preserve_the_declared_range() {
        for value in [0, 1, 99, 100] {
            let percent = Percent::try_from(value).unwrap();
            assert_eq!(u16::from(percent), value);
        }
        for value in [101, 300, u16::MAX] {
            assert_eq!(
                Percent::try_from(value),
                Err(PercentError::OutOfRange(value))
            );
        }
    }

    #[test]
    fn parsing_and_display_have_explicit_normalization() {
        let percent: Percent = "003".parse().unwrap();
        assert_eq!(percent.to_string(), "3");
        assert_eq!(percent.to_string().parse::<Percent>(), Ok(percent));
        for bad in ["", "+3", "-1", "1.0", " 3", "3 ", "0000", "３"] {
            assert_eq!(bad.parse::<Percent>(), Err(PercentError::Syntax));
        }
        assert_eq!("101".parse::<Percent>(), Err(PercentError::OutOfRange(101)));
    }

    #[test]
    fn collection_reports_first_failure_instead_of_dropping_it() {
        assert_eq!(
            parse_percentages(&["25", "101", "bad"]),
            Err(PercentError::OutOfRange(101))
        );
        assert_eq!(
            parse_percentages(&["bad", "101"]),
            Err(PercentError::Syntax)
        );
        assert_eq!(parse_percentages(&[]), Ok(vec![]));
    }

    #[test]
    fn filter_is_inclusive_and_preserves_order_and_duplicates() {
        let values = parse_percentages(&["100", "24", "25", "25", "0"]).unwrap();
        assert_eq!(
            selected_basis_points(&values, Percent::try_from(25).unwrap()),
            [10000, 2500, 2500]
        );
        assert!(selected_basis_points(&[], Percent::try_from(0).unwrap()).is_empty());
    }
}
