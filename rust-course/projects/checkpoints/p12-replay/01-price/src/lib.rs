//! Exact four-place price representation before replay state transitions.
use std::fmt;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Price(u64);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvalidPrice;
impl fmt::Display for InvalidPrice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("expected a positive decimal with at most four fractional places and at most 100000000 whole units")
    }
}
impl std::error::Error for InvalidPrice {}
impl Price {
    pub const SCALE: u64 = 10_000;
    pub fn parse(input: &str) -> Result<Self, InvalidPrice> {
        let (whole, fractional) = input.split_once('.').unwrap_or((input, ""));
        if whole.is_empty()
            || !whole.bytes().all(|b| b.is_ascii_digit())
            || fractional.len() > 4
            || !fractional.bytes().all(|b| b.is_ascii_digit())
        {
            return Err(InvalidPrice);
        }
        let whole = whole.parse::<u64>().map_err(|_| InvalidPrice)?;
        let mut fraction = if fractional.is_empty() {
            0
        } else {
            fractional.parse::<u64>().map_err(|_| InvalidPrice)?
        };
        for _ in fractional.len()..4 {
            fraction *= 10;
        }
        let ticks = whole
            .checked_mul(Self::SCALE)
            .and_then(|n| n.checked_add(fraction))
            .ok_or(InvalidPrice)?;
        if ticks == 0 || ticks > 1_000_000_000_000 {
            return Err(InvalidPrice);
        }
        Ok(Self(ticks))
    }
    pub fn ticks(self) -> u64 {
        self.0
    }
}
impl fmt::Display for Price {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{:04}", self.0 / Self::SCALE, self.0 % Self::SCALE)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_scale_and_boundaries() {
        for (text, ticks) in [
            ("12.34", 123400),
            ("0.0001", 1),
            ("100000000", 1_000_000_000_000),
            ("1.", 10000),
        ] {
            assert_eq!(Price::parse(text).unwrap().ticks(), ticks);
        }
    }
    #[test]
    fn rejects_invalid_grammar_precision_zero_and_range() {
        for text in [
            "",
            "0",
            "-1",
            "+1",
            "NaN",
            "1e2",
            "1.00001",
            "1.2.3",
            "100000001",
            "999999999999999999999999",
        ] {
            assert_eq!(Price::parse(text), Err(InvalidPrice), "{text}");
        }
    }
    #[test]
    fn formatted_valid_prices_round_trip_without_float() {
        for whole in 0..32 {
            for fraction in [0, 1, 99, 9999] {
                if whole == 0 && fraction == 0 {
                    continue;
                }
                let text = format!("{whole}.{fraction:04}");
                let price = Price::parse(&text).unwrap();
                assert_eq!(Price::parse(&price.to_string()), Ok(price));
                assert_eq!(price.ticks(), whole * 10000 + fraction);
            }
        }
    }
}
