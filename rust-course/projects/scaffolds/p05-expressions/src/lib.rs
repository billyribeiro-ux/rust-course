//! Learner task: lexer + precedence parser + checked evaluator, with limits.
#[derive(Debug, PartialEq, Eq)]
pub struct Error {
    pub start: usize,
    pub end: usize,
    pub message: &'static str,
}
pub fn evaluate(source: &str) -> Result<i64, Error> {
    let _ = source;
    todo!("preserve byte spans, left associativity, unary minus and resource limits")
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn precedence() {
        assert_eq!(evaluate("2+3*4"), Ok(14));
        assert_eq!(evaluate("(2+3)*4"), Ok(20));
    }
    #[test]
    fn associativity() {
        assert_eq!(evaluate("20-5-3"), Ok(12));
    }
    #[test]
    fn unary() {
        assert_eq!(evaluate("-3*-2"), Ok(6));
    }
    #[test]
    fn spans() {
        let e = evaluate("8 / 0").unwrap_err();
        assert_eq!((e.start, e.end), (2, 3));
    }
    #[test]
    fn malformed() {
        assert!(evaluate("1 2").is_err());
        assert!(evaluate(&"(".repeat(100)).is_err());
    }
}
