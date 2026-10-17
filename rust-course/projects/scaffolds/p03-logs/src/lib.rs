//! Intentionally incomplete learner scaffold. Implement one contract at a time.
//! Keep input, line retention, query size and output failure policies explicit.
use std::io::{self, BufRead, Write};

#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub max_line_bytes: usize,
    pub max_total_bytes: u64,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            max_line_bytes: 4096,
            max_total_bytes: 64 * 1024 * 1024,
        }
    }
}
#[derive(Default, Debug, PartialEq, Eq)]
pub struct Report {
    pub lines: u64,
    pub valid: u64,
    pub malformed: u64,
    pub oversized: u64,
    pub info: u64,
    pub warnings: u64,
    pub errors: u64,
}
#[derive(Debug, PartialEq, Eq)]
pub struct SearchReport {
    pub summary: Report,
    pub matched: u64,
}
pub fn analyze(reader: impl BufRead, limits: Limits) -> io::Result<Report> {
    let _ = (reader, limits);
    todo!("bounded fill_buf/consume, drain oversize, validate UTF-8/grammar, count every line")
}
pub fn search(
    reader: impl BufRead,
    limits: Limits,
    query: &str,
    output: &mut impl Write,
) -> io::Result<SearchReport> {
    let _ = (reader, limits, query, output);
    todo!(
        "validate query; summarize every line; emit only valid literal matches; preserve I/O errors"
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn valid_and_bad() {
        let report = analyze(&b"1 INFO api ok\nbad\n"[..], Limits::default()).unwrap();
        assert_eq!((report.lines, report.valid, report.malformed), (2, 1, 1));
    }
    #[test]
    fn drain() {
        let report = analyze(
            &b"xxxxxxxxxxxxxxxxxxxx\n1 INFO x ok\n"[..],
            Limits {
                max_line_bytes: 12,
                ..Limits::default()
            },
        )
        .unwrap();
        assert_eq!((report.lines, report.valid, report.oversized), (2, 1, 1));
    }
    #[test]
    fn missing_final_newline() {
        assert_eq!(
            analyze(&b"1 WARN api slow"[..], Limits::default())
                .unwrap()
                .valid,
            1
        );
    }
    #[test]
    fn invalid_utf8() {
        assert_eq!(
            analyze(&b"\xff\n"[..], Limits::default())
                .unwrap()
                .malformed,
            1
        );
    }
    #[test]
    fn utf8_search_does_not_change_summary_accounting() {
        let mut output = Vec::new();
        let result = search(
            "1 INFO api café\r\n2 INFO api tea".as_bytes(),
            Limits::default(),
            "café",
            &mut output,
        )
        .unwrap();
        assert_eq!((result.summary.valid, result.matched), (2, 1));
        assert_eq!(output, "1 INFO api café\n".as_bytes());
    }
    #[test]
    fn budget_is_a_failure_not_an_incomplete_success() {
        let error = analyze(
            &b"1 INFO api ready\n"[..],
            Limits {
                max_total_bytes: 3,
                ..Limits::default()
            },
        )
        .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    }
}
