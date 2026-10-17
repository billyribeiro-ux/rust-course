//! An integer expression language with byte spans and explicit resource limits.
//! ```
//! assert_eq!(expression_language::evaluate("2 + 3 * 4")?, 14);
//! assert_eq!(expression_language::evaluate("(2 + 3) * 4")?, 20);
//! assert!(expression_language::evaluate("8 / 0").is_err());
//! # Ok::<(), expression_language::Error>(())
//! ```
use std::fmt;
pub const MAX_SOURCE_BYTES: usize = 4096;
pub const MAX_TOKENS: usize = 512;
pub const MAX_DEPTH: usize = 64;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Error {
    pub span: Span,
    pub message: &'static str,
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} at bytes {}..{}",
            self.message, self.span.start, self.span.end
        )
    }
}
impl std::error::Error for Error {}
fn error(span: Span, message: &'static str) -> Error {
    Error { span, message }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Number(i64),
    Plus,
    Minus,
    Star,
    Slash,
    Open,
    Close,
}
#[derive(Clone, Copy, Debug)]
struct Token {
    kind: Kind,
    span: Span,
}
fn lex(source: &str) -> Result<Vec<Token>, Error> {
    if source.len() > MAX_SOURCE_BYTES {
        return Err(error(
            Span {
                start: 0,
                end: source.len(),
            },
            "source byte limit exceeded",
        ));
    }
    let bytes = source.as_bytes();
    let mut index = 0;
    let mut tokens = Vec::new();
    while index < bytes.len() {
        if bytes[index].is_ascii_whitespace() {
            index += 1;
            continue;
        }
        let start = index;
        let kind = match bytes[index] {
            b'0'..=b'9' => {
                while index < bytes.len() && bytes[index].is_ascii_digit() {
                    index += 1;
                }
                let value = source[start..index]
                    .parse()
                    .map_err(|_| error(Span { start, end: index }, "integer literal overflow"))?;
                Kind::Number(value)
            }
            byte => {
                index += 1;
                match byte {
                    b'+' => Kind::Plus,
                    b'-' => Kind::Minus,
                    b'*' => Kind::Star,
                    b'/' => Kind::Slash,
                    b'(' => Kind::Open,
                    b')' => Kind::Close,
                    _ => {
                        return Err(error(
                            Span {
                                start,
                                end: start
                                    + source[start..]
                                        .chars()
                                        .next()
                                        .expect("start is a remaining UTF-8 character boundary")
                                        .len_utf8(),
                            },
                            "unexpected character",
                        ));
                    }
                }
            }
        };
        if tokens.len() == MAX_TOKENS {
            return Err(error(Span { start, end: index }, "token limit exceeded"));
        }
        tokens.push(Token {
            kind,
            span: Span { start, end: index },
        });
    }
    Ok(tokens)
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Operator {
    Add,
    Subtract,
    Multiply,
    Divide,
}
#[derive(Debug)]
enum Node {
    Number(i64),
    Negate {
        operand: Box<Expr>,
        operator_span: Span,
    },
    Binary {
        left: Box<Expr>,
        op: Operator,
        right: Box<Expr>,
        operator_span: Span,
    },
}
/// Fields are private so callers cannot bypass the parser's depth invariant.
#[derive(Debug)]
pub struct Expr {
    node: Node,
    span: Span,
    depth: usize,
}
impl Expr {
    pub fn span(&self) -> Span {
        self.span
    }
    pub fn depth(&self) -> usize {
        self.depth
    }
    pub fn evaluate(&self) -> Result<i64, Error> {
        match &self.node {
            Node::Number(value) => Ok(*value),
            Node::Negate {
                operand,
                operator_span,
            } => operand
                .evaluate()?
                .checked_neg()
                .ok_or_else(|| error(*operator_span, "arithmetic overflow")),
            Node::Binary {
                left,
                op,
                right,
                operator_span,
            } => {
                let left = left.evaluate()?;
                let right = right.evaluate()?;
                let value = match op {
                    Operator::Add => left.checked_add(right),
                    Operator::Subtract => left.checked_sub(right),
                    Operator::Multiply => left.checked_mul(right),
                    Operator::Divide => {
                        if right == 0 {
                            return Err(error(*operator_span, "division by zero"));
                        }
                        left.checked_div(right)
                    }
                };
                value.ok_or_else(|| error(*operator_span, "arithmetic overflow"))
            }
        }
    }
}
struct Parser {
    tokens: Vec<Token>,
    cursor: usize,
    source_len: usize,
}
impl Parser {
    fn peek(&self) -> Option<Token> {
        self.tokens.get(self.cursor).copied()
    }
    fn next(&mut self) -> Option<Token> {
        let token = self.peek();
        if token.is_some() {
            self.cursor += 1;
        }
        token
    }
    fn current_span(&self) -> Span {
        self.peek().map_or(
            Span {
                start: self.source_len,
                end: self.source_len,
            },
            |t| t.span,
        )
    }
    fn expression(&mut self, minimum: u8, nesting: usize) -> Result<Expr, Error> {
        if nesting >= MAX_DEPTH {
            return Err(error(self.current_span(), "nesting limit exceeded"));
        }
        let token = self
            .next()
            .ok_or_else(|| error(self.current_span(), "expected an expression"))?;
        let mut left = match token.kind {
            Kind::Number(value) => Expr {
                node: Node::Number(value),
                span: token.span,
                depth: 1,
            },
            Kind::Minus => {
                let operand = self.expression(3, nesting + 1)?;
                let depth = operand.depth + 1;
                if depth > MAX_DEPTH {
                    return Err(error(token.span, "AST depth limit exceeded"));
                }
                Expr {
                    span: Span {
                        start: token.span.start,
                        end: operand.span.end,
                    },
                    depth,
                    node: Node::Negate {
                        operand: Box::new(operand),
                        operator_span: token.span,
                    },
                }
            }
            Kind::Open => {
                let mut inner = self.expression(0, nesting + 1)?;
                let close = self
                    .next()
                    .ok_or_else(|| error(self.current_span(), "expected closing parenthesis"))?;
                if close.kind != Kind::Close {
                    return Err(error(close.span, "expected closing parenthesis"));
                }
                inner.span = Span {
                    start: token.span.start,
                    end: close.span.end,
                };
                inner
            }
            _ => {
                return Err(error(
                    token.span,
                    "expected a number, minus, or opening parenthesis",
                ));
            }
        };
        while let Some(token) = self.peek() {
            let (precedence, op) = match token.kind {
                Kind::Plus => (1, Operator::Add),
                Kind::Minus => (1, Operator::Subtract),
                Kind::Star => (2, Operator::Multiply),
                Kind::Slash => (2, Operator::Divide),
                _ => break,
            };
            if precedence < minimum {
                break;
            }
            self.next();
            let right = self.expression(precedence + 1, nesting + 1)?;
            let depth = 1 + left.depth.max(right.depth);
            if depth > MAX_DEPTH {
                return Err(error(token.span, "AST depth limit exceeded"));
            }
            left = Expr {
                span: Span {
                    start: left.span.start,
                    end: right.span.end,
                },
                depth,
                node: Node::Binary {
                    left: Box::new(left),
                    op,
                    right: Box::new(right),
                    operator_span: token.span,
                },
            };
        }
        Ok(left)
    }
}
pub fn parse(source: &str) -> Result<Expr, Error> {
    let mut parser = Parser {
        tokens: lex(source)?,
        cursor: 0,
        source_len: source.len(),
    };
    let expression = parser.expression(0, 0)?;
    if parser.peek().is_some() {
        return Err(error(parser.current_span(), "unexpected trailing token"));
    }
    Ok(expression)
}
pub fn evaluate(source: &str) -> Result<i64, Error> {
    parse(source)?.evaluate()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn precedence_associativity_unary() {
        for (source, value) in [
            ("2 + 3 * 4", 14),
            ("(2 + 3) * 4", 20),
            ("20 - 5 - 3", 12),
            ("24 / 4 / 2", 3),
            ("-3 * -2", 6),
            ("--3", 3),
            ("-7/2", -3),
        ] {
            assert_eq!(evaluate(source).unwrap(), value, "{source}");
        }
    }
    #[test]
    fn precise_error_spans() {
        let e = evaluate("8 / 0").unwrap_err();
        assert_eq!(e.span, Span { start: 2, end: 3 });
        assert_eq!(e.message, "division by zero");
        let e = parse("2 + 🦀").unwrap_err();
        assert_eq!(e.span, Span { start: 4, end: 8 });
        for source in [
            "",
            "1 2",
            "(1",
            "1)",
            "+1",
            "1+",
            "9223372036854775808",
            "9223372036854775807+1",
        ] {
            assert!(evaluate(source).is_err(), "{source}");
        }
    }
    #[test]
    fn every_arithmetic_overflow_reports_its_operator() {
        // Form MIN by subtraction because unsigned integer tokens cannot hold 2^63.
        // These boundary checks must behave identically in debug and release builds.
        for (source, start) in [
            ("9223372036854775807 + 1", 20),
            ("(-9223372036854775807 - 1) - 1", 27),
            ("9223372036854775807 * 2", 20),
            ("-(-9223372036854775807 - 1)", 0),
            ("(-9223372036854775807 - 1) / -1", 27),
        ] {
            assert_eq!(
                evaluate(source),
                Err(Error {
                    span: Span {
                        start,
                        end: start + 1
                    },
                    message: "arithmetic overflow",
                }),
                "{source}"
            );
        }
        for (source, expected) in [
            ("9223372036854775807 + 0", i64::MAX),
            ("-9223372036854775807 - 1", i64::MIN),
            ("9223372036854775807 * 1", i64::MAX),
            ("(-9223372036854775807 - 1) / 1", i64::MIN),
            ("-(-9223372036854775807)", i64::MAX),
        ] {
            assert_eq!(evaluate(source), Ok(expected), "{source}");
        }
    }
    #[test]
    fn resource_limits_are_errors() {
        assert!(parse(&"(".repeat(100)).is_err());
        assert!(parse(&"-".repeat(100)).is_err());
        assert!(parse(&vec!["1"; 100].join("+")).is_err());
        assert!(parse(&vec!["1"; 300].join("+")).is_err());
        assert!(parse(&" ".repeat(MAX_SOURCE_BYTES + 1)).is_err());
    }
    #[test]
    fn exact_byte_and_token_limits_have_distinct_diagnostics() {
        let at_bytes = format!("1{}", " ".repeat(MAX_SOURCE_BYTES - 1));
        assert_eq!(evaluate(&at_bytes), Ok(1));
        let over_bytes = format!("{at_bytes} ");
        assert_eq!(
            parse(&over_bytes).unwrap_err(),
            error(
                Span {
                    start: 0,
                    end: MAX_SOURCE_BYTES + 1
                },
                "source byte limit exceeded"
            )
        );

        // 128 leaves give 127 operators and 254 parentheses: 509 tokens.
        // Three unary minuses reach 512 without approaching either depth limit.
        let mut balanced = "1".to_owned();
        for _ in 0..7 {
            balanced = format!("({balanced}+{balanced})");
        }
        let at_tokens = format!("---{balanced}");
        assert_eq!(lex(&at_tokens).unwrap().len(), MAX_TOKENS);
        assert_eq!(evaluate(&at_tokens), Ok(-128));
        let over_tokens = format!("-{at_tokens}");
        assert_eq!(
            parse(&over_tokens).unwrap_err(),
            error(
                Span {
                    start: 512,
                    end: 513
                },
                "token limit exceeded"
            )
        );
    }
    #[test]
    fn recursive_nesting_and_constructed_tree_depth_are_separate_limits() {
        // Top-level expression is recursion level one. 63 pairs use 64 calls,
        // but parentheses do not add AST nodes: the resulting tree has depth 1.
        let at_nesting = format!("{}1{}", "(".repeat(63), ")".repeat(63));
        assert_eq!(parse(&at_nesting).unwrap().depth(), 1);
        let over_nesting = format!("({at_nesting})");
        assert_eq!(
            parse(&over_nesting).unwrap_err(),
            error(Span { start: 64, end: 65 }, "nesting limit exceeded")
        );

        // A flat left-associative chain uses shallow parser recursion while its
        // result gets one level deeper per number. It exercises the other guard.
        let at_depth = vec!["1"; MAX_DEPTH].join("+");
        let tree = parse(&at_depth).unwrap();
        assert_eq!(tree.depth(), MAX_DEPTH);
        assert_eq!(tree.evaluate(), Ok(64));
        assert_eq!(
            parse(&format!("{at_depth}+1")).unwrap_err(),
            error(
                Span {
                    start: 127,
                    end: 128
                },
                "AST depth limit exceeded"
            )
        );
    }
    #[test]
    fn deterministic_exhaustive_small_integer_properties() {
        // Finite exhaustive domain, not randomized fuzzing and not a proof for all i64.
        for a in -8_i64..=8 {
            for b in -8_i64..=8 {
                for c in -4_i64..=4 {
                    assert_eq!(evaluate(&format!("({a})+({b})*({c})")).unwrap(), a + b * c);
                    assert_eq!(
                        evaluate(&format!("(({a})+({b}))*({c})")).unwrap(),
                        (a + b) * c
                    );
                    assert_eq!(
                        evaluate(&format!("({a})+({b})")).unwrap(),
                        evaluate(&format!("({b})+({a})")).unwrap()
                    );
                    // Subtraction distinguishes left from right associativity.
                    // Parenthesizing each operand does not group the operations.
                    assert_eq!(
                        evaluate(&format!("({a})-({b})-({c})")).unwrap(),
                        (a - b) - c
                    );
                }
            }
        }
    }
}
