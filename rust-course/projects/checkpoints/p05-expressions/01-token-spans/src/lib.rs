use std::fmt;
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind {
    Number(i64),
    Plus,
    Star,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub kind: Kind,
    pub start: usize,
    pub end: usize,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    pub start: usize,
    pub end: usize,
    pub reason: &'static str,
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} at {}..{}", self.reason, self.start, self.end)
    }
}
impl std::error::Error for Error {}
pub fn lex(source: &str) -> Result<Vec<Token>, Error> {
    if source.len() > 1024 {
        return Err(Error {
            start: 0,
            end: source.len(),
            reason: "source limit",
        });
    }
    let bytes = source.as_bytes();
    let mut position = 0;
    let mut tokens = Vec::new();
    while position < bytes.len() {
        if bytes[position].is_ascii_whitespace() {
            position += 1;
            continue;
        }
        if tokens.len() == 64 {
            return Err(Error {
                start: position,
                end: position,
                reason: "token limit",
            });
        }
        let start = position;
        let kind = match bytes[position] {
            b'0'..=b'9' => {
                while position < bytes.len() && bytes[position].is_ascii_digit() {
                    position += 1;
                }
                Kind::Number(source[start..position].parse().map_err(|_| Error {
                    start,
                    end: position,
                    reason: "integer range",
                })?)
            }
            b'+' => {
                position += 1;
                Kind::Plus
            }
            b'*' => {
                position += 1;
                Kind::Star
            }
            _ => {
                let width = source[position..]
                    .chars()
                    .next()
                    .expect("position is a nonempty UTF-8 boundary")
                    .len_utf8();
                return Err(Error {
                    start,
                    end: start + width,
                    reason: "unknown symbol",
                });
            }
        };
        tokens.push(Token {
            kind,
            start,
            end: position,
        });
    }
    Ok(tokens)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn spans_follow_original_bytes() {
        assert_eq!(
            lex("12 + 3*4").unwrap(),
            vec![
                Token {
                    kind: Kind::Number(12),
                    start: 0,
                    end: 2
                },
                Token {
                    kind: Kind::Plus,
                    start: 3,
                    end: 4
                },
                Token {
                    kind: Kind::Number(3),
                    start: 5,
                    end: 6
                },
                Token {
                    kind: Kind::Star,
                    start: 6,
                    end: 7
                },
                Token {
                    kind: Kind::Number(4),
                    start: 7,
                    end: 8
                }
            ]
        );
    }
    #[test]
    fn unicode_error_covers_one_scalar() {
        let error = lex("1+é").unwrap_err();
        assert_eq!((error.start, error.end), (2, 4));
    }
    #[test]
    fn limits_and_integer_range() {
        assert!(lex(&"+".repeat(64)).is_ok());
        assert_eq!(lex(&"+".repeat(65)).unwrap_err().reason, "token limit");
        assert_eq!(lex(&" ".repeat(1025)).unwrap_err().reason, "source limit");
        assert_eq!(
            lex("9223372036854775808").unwrap_err().reason,
            "integer range"
        );
    }
    #[test]
    fn lexing_does_not_claim_syntax_validity() {
        assert_eq!(lex("1 2").unwrap().len(), 2);
        assert!(lex("").unwrap().is_empty());
        assert!(lex("-1").is_err());
    }
}
