//! Learner task: implement the contracts below. Tests intentionally fail until completed.
#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    Parse,
    Overflow,
    Zero,
    Units,
}
/// Accept signed integers with optional m/s suffix; implement +, -, scalar *, and /.
pub fn calculate(left: &str, operator: &str, right: &str) -> Result<String, Error> {
    let _ = (left, operator, right);
    todo!("parse quantities, validate units, then use checked arithmetic")
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sum() {
        assert_eq!(calculate("2m", "+", "3m"), Ok("5m".into()));
    }
    #[test]
    fn unit_error() {
        assert_eq!(calculate("2m", "+", "3s"), Err(Error::Units));
    }
    #[test]
    fn zero_error() {
        assert_eq!(calculate("2", "/", "0"), Err(Error::Zero));
    }
    #[test]
    fn overflow() {
        assert_eq!(
            calculate("9223372036854775807", "+", "1"),
            Err(Error::Overflow)
        );
    }
}
