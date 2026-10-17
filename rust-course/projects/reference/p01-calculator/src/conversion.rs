//! Approximate floating-point length conversion, separate from integer arithmetic.
//!
//! ```
//! use workshop_calculator::conversion::{convert, LengthUnit};
//! let converted = convert("1.5m", "cm")?;
//! assert_eq!(converted.value(), 150.0);
//! assert_eq!(converted.unit(), LengthUnit::Centimeters);
//! # Ok::<(), workshop_calculator::conversion::ConversionError>(())
//! ```
use std::fmt;

pub const MAX_INPUT_BYTES: usize = 128;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LengthUnit {
    Millimeters,
    Centimeters,
    Meters,
    Kilometers,
}

impl LengthUnit {
    pub fn parse(input: &str) -> Result<Self, ConversionError> {
        match input {
            "mm" => Ok(Self::Millimeters),
            "cm" => Ok(Self::Centimeters),
            "m" => Ok(Self::Meters),
            "km" => Ok(Self::Kilometers),
            _ => Err(ConversionError::InvalidUnit),
        }
    }

    pub fn symbol(self) -> &'static str {
        match self {
            Self::Millimeters => "mm",
            Self::Centimeters => "cm",
            Self::Meters => "m",
            Self::Kilometers => "km",
        }
    }

    // All four integer factors are exactly representable in f64. Their ratios
    // can still be inexact, so the result is explicitly an approximation.
    fn millimeters(self) -> f64 {
        match self {
            Self::Millimeters => 1.0,
            Self::Centimeters => 10.0,
            Self::Meters => 1000.0,
            Self::Kilometers => 1_000_000.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConversionError {
    InputTooLong,
    InvalidNumber,
    InvalidUnit,
    NonFinite,
    Overflow,
    Underflow,
}

impl fmt::Display for ConversionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InputTooLong => "conversion input exceeds 128 bytes",
            Self::InvalidNumber => "expected a number immediately followed by mm, cm, m, or km",
            Self::InvalidUnit => "length unit must be mm, cm, m, or km",
            Self::NonFinite => "input must be finite; NaN and infinity are not lengths",
            Self::Overflow => "converted value exceeds the finite f64 range",
            Self::Underflow => {
                "a nonzero value rounded to zero; choose a different unit or representation"
            }
        })
    }
}

impl std::error::Error for ConversionError {}

/// Validated finite value and recognized length unit. Fields stay private so
/// conversion callers cannot bypass finite-value validation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Length {
    value: f64,
    unit: LengthUnit,
}

impl Length {
    pub fn value(self) -> f64 {
        self.value
    }

    pub fn unit(self) -> LengthUnit {
        self.unit
    }

    pub fn parse(input: &str) -> Result<Self, ConversionError> {
        if input.len() > MAX_INPUT_BYTES {
            return Err(ConversionError::InputTooLong);
        }
        // Longest suffixes first: "1mm" must not become the number "1m".
        let (number, unit) = [
            LengthUnit::Millimeters,
            LengthUnit::Centimeters,
            LengthUnit::Kilometers,
            LengthUnit::Meters,
        ]
        .into_iter()
        .find_map(|unit| {
            input
                .strip_suffix(unit.symbol())
                .map(|number| (number, unit))
        })
        .ok_or(ConversionError::InvalidUnit)?;
        if number.is_empty() || number.trim() != number {
            return Err(ConversionError::InvalidNumber);
        }
        let value = number
            .parse::<f64>()
            .map_err(|_| ConversionError::InvalidNumber)?;
        if !value.is_finite() {
            return Err(ConversionError::NonFinite);
        }
        // Parsing an extremely tiny decimal can already round a nonzero input
        // to zero. Exponent digits do not determine whether the significand is zero.
        let significand = number.split(['e', 'E']).next().unwrap_or(number);
        if value == 0.0
            && significand
                .bytes()
                .any(|digit| matches!(digit, b'1'..=b'9'))
        {
            return Err(ConversionError::Underflow);
        }
        Ok(Self { value, unit })
    }

    pub fn to_unit(self, unit: LengthUnit) -> Result<Self, ConversionError> {
        let ratio = self.unit.millimeters() / unit.millimeters();
        // Scale once. (value * source_factor) / target_factor can overflow at
        // the intermediate product even when the requested result is finite.
        let value = self.value * ratio;
        if !value.is_finite() {
            return Err(ConversionError::Overflow);
        }
        if self.value != 0.0 && value == 0.0 {
            return Err(ConversionError::Underflow);
        }
        Ok(Self { value, unit })
    }
}

pub fn convert(input: &str, target_unit: &str) -> Result<Length, ConversionError> {
    Length::parse(input)?.to_unit(LengthUnit::parse(target_unit)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn common_fractional_signed_and_scientific_values() {
        for (input, target, expected) in [
            ("1.5m", "cm", 150.0),
            ("-2.5km", "m", -2500.0),
            ("+0.125m", "mm", 125.0),
            ("1.25e3mm", "m", 1.25),
            ("2.5E-2km", "m", 25.0),
        ] {
            let actual = convert(input, target).unwrap();
            assert_eq!(actual.value(), expected, "{input} -> {target}");
            assert_eq!(actual.unit().symbol(), target);
        }
    }

    #[test]
    fn every_unit_pair_agrees_with_independent_expected_values() {
        // Explicit expected conversions avoid deriving the oracle through the
        // implementation's factor-division algorithm.
        let cases = [
            ("1mm", [1.0, 0.1, 0.001, 0.000_001]),
            ("1cm", [10.0, 1.0, 0.01, 0.000_01]),
            ("1m", [1000.0, 100.0, 1.0, 0.001]),
            ("1km", [1_000_000.0, 100_000.0, 1000.0, 1.0]),
        ];
        for (input, expected_values) in cases {
            for (target, expected) in ["mm", "cm", "m", "km"].into_iter().zip(expected_values) {
                let actual = convert(input, target).unwrap().value();
                assert!(
                    (actual - expected).abs() <= expected * 1e-14,
                    "{input} -> {target}"
                );
            }
        }
    }

    #[test]
    fn invalid_units_numbers_and_nonfinite_inputs_are_errors() {
        for input in ["NaNm", "infm", "-infm", "1e309m"] {
            assert_eq!(
                convert(input, "cm"),
                Err(ConversionError::NonFinite),
                "{input}"
            );
        }
        for input in ["1m ", "1s", "1MM", "m", " 1m", "1 m", "1_000m", "1.2.3m"] {
            assert!(convert(input, "cm").is_err(), "{input}");
        }
        for target in ["", "s", "CM", " cm", "cm "] {
            assert_eq!(convert("1m", target), Err(ConversionError::InvalidUnit));
        }
        assert_eq!(
            convert(&format!("{}m", "1".repeat(128)), "m"),
            Err(ConversionError::InputTooLong)
        );
        let longest_accepted = format!("{}1m", "0".repeat(126));
        assert_eq!(longest_accepted.len(), MAX_INPUT_BYTES);
        assert_eq!(convert(&longest_accepted, "m").unwrap().value(), 1.0);
    }

    #[test]
    fn avoids_false_intermediate_overflow_and_rejects_real_range_loss() {
        assert_eq!(convert("1e308km", "km").unwrap().value(), 1e308);
        assert_eq!(convert("1e308m", "km").unwrap().value(), 1e305);
        assert_eq!(convert("1e308km", "mm"), Err(ConversionError::Overflow));
        assert_eq!(convert("-1e308km", "mm"), Err(ConversionError::Overflow));
        assert_eq!(convert("5e-324mm", "km"), Err(ConversionError::Underflow));
        assert_eq!(convert("1e-999m", "cm"), Err(ConversionError::Underflow));
        assert!(convert("1e-300m", "mm").unwrap().value().is_finite());
        assert!(convert("5e-324m", "mm").unwrap().value() > 0.0);
    }

    #[test]
    fn explicit_zero_is_valid_and_negative_zero_keeps_its_sign() {
        for input in ["0m", "0.0e-999m", "+0cm"] {
            assert_eq!(convert(input, "km").unwrap().value(), 0.0);
        }
        let negative_zero = convert("-0.0m", "cm").unwrap().value();
        assert_eq!(negative_zero, 0.0);
        assert!(negative_zero.is_sign_negative());
    }
}
