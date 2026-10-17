//! Checked integer arithmetic with a deliberately small unit system.
//! ```
//! use workshop_calculator::calculate;
//! let result = calculate("12m", "+", "3m")?;
//! assert_eq!(result.to_string(), "15m");
//! assert!(calculate("12m", "+", "3s").is_err());
//! # Ok::<(), workshop_calculator::CalcError>(())
//! ```
use std::{fmt, str::FromStr};

pub mod conversion;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unit {
    Scalar,
    Meters,
    Seconds,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Quantity {
    pub value: i64,
    pub unit: Unit,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CalcError {
    InvalidNumber,
    UnknownOperator,
    IncompatibleUnits,
    Overflow,
    DivisionByZero,
}
impl fmt::Display for CalcError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidNumber => "expected an integer optionally followed by m or s",
            Self::UnknownOperator => "operator must be +, -, *, or /",
            Self::IncompatibleUnits => "units do not support this operation",
            Self::Overflow => "result is outside the i64 range",
            Self::DivisionByZero => "cannot divide by zero",
        })
    }
}
impl std::error::Error for CalcError {}
impl FromStr for Quantity {
    type Err = CalcError;
    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let (number, unit) = if let Some(s) = input.strip_suffix('m') {
            (s, Unit::Meters)
        } else if let Some(s) = input.strip_suffix('s') {
            (s, Unit::Seconds)
        } else {
            (input, Unit::Scalar)
        };
        if number.is_empty() || number.trim() != number {
            return Err(CalcError::InvalidNumber);
        }
        Ok(Self {
            value: number.parse().map_err(|_| CalcError::InvalidNumber)?,
            unit,
        })
    }
}
impl fmt::Display for Quantity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let suffix = match self.unit {
            Unit::Scalar => "",
            Unit::Meters => "m",
            Unit::Seconds => "s",
        };
        write!(f, "{}{suffix}", self.value)
    }
}
impl Quantity {
    /// Integer division truncates toward zero. Products of two non-scalar units
    /// are deliberately unsupported; this is not a full dimensional algebra.
    pub fn apply(self, op: &str, rhs: Self) -> Result<Self, CalcError> {
        use CalcError::*;
        let (value, unit) = match op {
            "+" | "-" => {
                if self.unit != rhs.unit {
                    return Err(IncompatibleUnits);
                }
                (
                    if op == "+" {
                        self.value.checked_add(rhs.value)
                    } else {
                        self.value.checked_sub(rhs.value)
                    },
                    self.unit,
                )
            }
            "*" => {
                let unit = if self.unit == Unit::Scalar {
                    rhs.unit
                } else if rhs.unit == Unit::Scalar {
                    self.unit
                } else {
                    return Err(IncompatibleUnits);
                };
                (self.value.checked_mul(rhs.value), unit)
            }
            "/" => {
                let unit = if rhs.unit == Unit::Scalar {
                    self.unit
                } else if self.unit == rhs.unit {
                    Unit::Scalar
                } else {
                    return Err(IncompatibleUnits);
                };
                if rhs.value == 0 {
                    return Err(DivisionByZero);
                }
                (self.value.checked_div(rhs.value), unit)
            }
            _ => return Err(UnknownOperator),
        };
        Ok(Self {
            value: value.ok_or(Overflow)?,
            unit,
        })
    }
}
pub fn calculate(left: &str, operator: &str, right: &str) -> Result<Quantity, CalcError> {
    left.parse::<Quantity>()?.apply(operator, right.parse()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn units_and_integer_division() {
        for (a, op, b, out) in [
            ("12m", "+", "3m", "15m"),
            ("3", "*", "4s", "12s"),
            ("7m", "/", "2m", "3"),
            ("-7", "/", "2", "-3"),
        ] {
            assert_eq!(calculate(a, op, b).unwrap().to_string(), out);
        }
    }
    #[test]
    fn recoverable_errors() {
        assert_eq!(
            calculate("1m", "+", "1s"),
            Err(CalcError::IncompatibleUnits)
        );
        assert_eq!(
            calculate("2m", "*", "2m"),
            Err(CalcError::IncompatibleUnits)
        );
        assert_eq!(calculate("1", "/", "0"), Err(CalcError::DivisionByZero));
        assert_eq!(
            calculate("9223372036854775807", "+", "1"),
            Err(CalcError::Overflow)
        );
        assert_eq!(
            calculate("-9223372036854775808", "/", "-1"),
            Err(CalcError::Overflow)
        );
        assert_eq!(calculate("1.5", "+", "2"), Err(CalcError::InvalidNumber));
    }
    #[test]
    fn exhaustive_small_addition_commutes() {
        for a in -20..=20 {
            for b in -20..=20 {
                let a = Quantity {
                    value: a,
                    unit: Unit::Meters,
                };
                let b = Quantity {
                    value: b,
                    unit: Unit::Meters,
                };
                assert_eq!(a.apply("+", b), b.apply("+", a));
            }
        }
    }
}
