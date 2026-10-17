/// Parse positive decimal prices into 1/10,000 units, maximum 1,000,000,000,000 ticks.
/// Reject signs, exponents, nondigits, excess precision, zero and overflow.
pub fn price_ticks(_input: &str) -> Result<u64, &'static str> {
    todo!("Learner: validate grammar and use checked integer arithmetic; no floating point")
}
#[cfg(test)] mod acceptance {
    use super::*;
    #[test] fn exact_scale() { assert_eq!(price_ticks("12.34"), Ok(123400)); assert_eq!(price_ticks("0.0001"), Ok(1)); assert_eq!(price_ticks("100000000"), Ok(1_000_000_000_000)); }
    #[test] fn invalid_and_overflow() { for s in ["", "0", "-1", "+1", "NaN", "1e2", "1.00001", "1.2.3", "100000001", "999999999999999999999999"] { assert!(price_ticks(s).is_err(), "{s}"); } }
}
