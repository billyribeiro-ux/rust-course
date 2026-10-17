//! Bounded-memory, line-oriented log analysis with borrowed record fields.
//! ```
//! use log_detective::{analyze, Limits};
//! let report = analyze(&b"10 INFO api ready\n11 ERROR api unavailable\n"[..], Limits::default())?;
//! assert_eq!(report.errors, 1);
//! assert_eq!(report.valid, 2);
//! # Ok::<(), std::io::Error>(())
//! ```
use std::io::{self, BufRead, Write};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    Info,
    Warn,
    Error,
}
#[derive(Debug, PartialEq, Eq)]
pub struct Record<'a> {
    pub timestamp: u64,
    pub level: Level,
    pub service: &'a str,
    pub message: &'a str,
}
pub fn parse_record(input: &str) -> Result<Record<'_>, &'static str> {
    let mut fields = input.splitn(4, ' ');
    let timestamp = fields
        .next()
        .ok_or("timestamp missing")?
        .parse()
        .map_err(|_| "invalid timestamp")?;
    let level = match fields.next() {
        Some("INFO") => Level::Info,
        Some("WARN") => Level::Warn,
        Some("ERROR") => Level::Error,
        _ => return Err("invalid level"),
    };
    let service = fields.next().ok_or("service missing")?;
    if service.is_empty()
        || !service
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err("invalid service");
    }
    let message = fields.next().ok_or("message missing")?;
    if message.trim().is_empty() || message.chars().any(char::is_control) {
        return Err("invalid message");
    }
    Ok(Record {
        timestamp,
        level,
        service,
        message,
    })
}
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
enum Line {
    End,
    Data,
    Oversized,
}
fn read_bounded<R: BufRead>(
    reader: &mut R,
    output: &mut Vec<u8>,
    limit: usize,
    total: &mut u64,
    total_limit: u64,
) -> io::Result<Line> {
    output.clear();
    let mut seen = false;
    let mut oversized = false;
    loop {
        let available = match reader.fill_buf() {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        };
        if available.is_empty() {
            return Ok(if !seen {
                Line::End
            } else if oversized {
                Line::Oversized
            } else {
                Line::Data
            });
        }
        let newline = available.iter().position(|&b| b == b'\n');
        let take = newline.map_or(available.len(), |n| n + 1);
        *total = total
            .checked_add(take as u64)
            .filter(|&n| n <= total_limit)
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidData, "input byte limit exceeded")
            })?;
        let payload = if newline.is_some() { take - 1 } else { take };
        if !oversized {
            if payload > limit.saturating_sub(output.len()) {
                oversized = true;
                output.clear();
            } else {
                output.extend_from_slice(&available[..payload]);
            }
        }
        reader.consume(take);
        seen = true;
        if newline.is_some() {
            if oversized {
                return Ok(Line::Oversized);
            }
            if output.last() == Some(&b'\r') {
                output.pop();
            }
            return Ok(Line::Data);
        }
    }
}
/// At most `max_line_bytes` retained per line; an oversized line is drained.
/// Reader buffering is additional memory controlled by the caller.
pub fn analyze<R: BufRead>(reader: R, limits: Limits) -> io::Result<Report> {
    scan(reader, limits, None, &mut io::sink()).map(|result| result.summary)
}

/// Search valid records with a case-sensitive literal UTF-8 substring.
///
/// The query covers the complete normalized record (including its fields).
/// There is no case folding or Unicode normalization. Matching lines are written
/// immediately, with one LF terminator; CRLF input is normalized to LF.
/// Malformed and oversized lines are counted but never emitted. All valid
/// records contribute to the summary, whether or not they match.
///
/// Input/output errors abort the scan: the destination may already contain a
/// prefix, but no successful report is returned. The caller owns flushing.
/// ```
/// use log_detective::{search, Limits};
/// let mut output = Vec::new();
/// let result = search(&b"1 INFO api ready\n2 ERROR api retry\n"[..],
///     Limits::default(), "retry", &mut output)?;
/// assert_eq!(result.matched, 1);
/// assert_eq!(result.summary.valid, 2);
/// assert_eq!(output, b"2 ERROR api retry\n");
/// # Ok::<(), std::io::Error>(())
/// ```
pub fn search<R: BufRead, W: Write>(
    reader: R,
    limits: Limits,
    query: &str,
    output: &mut W,
) -> io::Result<SearchReport> {
    if query.is_empty() || query.len() > 4096 || query.chars().any(char::is_control) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "query must be 1..=4096 UTF-8 bytes without control characters",
        ));
    }
    scan(reader, limits, Some(query), output)
}

/// Fixed-cardinality counters: no collection grows with services or matches.
#[derive(Debug, PartialEq, Eq)]
pub struct SearchReport {
    pub summary: Report,
    pub matched: u64,
}

fn scan<R: BufRead, W: Write>(
    mut reader: R,
    limits: Limits,
    query: Option<&str>,
    output: &mut W,
) -> io::Result<SearchReport> {
    if limits.max_line_bytes == 0
        || limits.max_line_bytes > 1024 * 1024
        || limits.max_total_bytes == 0
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "line limit must be 1..=1048576 and total limit positive",
        ));
    }
    let mut report = Report::default();
    let mut matched = 0;
    let mut line = Vec::new();
    let mut total = 0;
    loop {
        match read_bounded(
            &mut reader,
            &mut line,
            limits.max_line_bytes,
            &mut total,
            limits.max_total_bytes,
        )? {
            Line::End => {
                return Ok(SearchReport {
                    summary: report,
                    matched,
                });
            }
            Line::Oversized => {
                report.lines += 1;
                report.oversized += 1;
            }
            Line::Data => {
                report.lines += 1;
                let text = std::str::from_utf8(&line).map_err(|_| "invalid UTF-8");
                match text.and_then(parse_record) {
                    Err(_) => report.malformed += 1,
                    Ok(record) => {
                        report.valid += 1;
                        match record.level {
                            Level::Info => report.info += 1,
                            Level::Warn => report.warnings += 1,
                            Level::Error => report.errors += 1,
                        }
                        // A valid record was parsed from this same UTF-8 line.
                        // Searching its full text never indexes into a character.
                        if let (Some(query), Ok(text)) = (query, text)
                            && text.contains(query)
                        {
                            output.write_all(text.as_bytes())?;
                            output.write_all(b"\n")?;
                            matched += 1;
                        }
                    }
                }
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufReader, Cursor};
    #[test]
    fn tiny_buffers_and_final_line() {
        let input = b"1 INFO api ready\r\n2 WARN db slow\n3 ERROR api failed";
        let report = analyze(
            BufReader::with_capacity(1, Cursor::new(input)),
            Limits::default(),
        )
        .unwrap();
        assert_eq!(
            report,
            Report {
                lines: 3,
                valid: 3,
                info: 1,
                warnings: 1,
                errors: 1,
                ..Report::default()
            }
        );
    }
    #[test]
    fn drain_oversized_then_recover() {
        let bytes = b"aaaaaaaaaaaaaaaaaaaaaa\n1 INFO x ok\n\xff\n";
        let report = analyze(
            BufReader::with_capacity(3, Cursor::new(bytes)),
            Limits {
                max_line_bytes: 12,
                max_total_bytes: 100,
            },
        )
        .unwrap();
        assert_eq!(
            (
                report.lines,
                report.oversized,
                report.valid,
                report.malformed
            ),
            (3, 1, 1, 1)
        );
    }
    #[test]
    fn rejects_total_exhaustion_and_invalid_limits() {
        assert!(
            analyze(
                &b"12345"[..],
                Limits {
                    max_line_bytes: 2,
                    max_total_bytes: 4
                }
            )
            .is_err()
        );
        assert!(
            analyze(
                &b""[..],
                Limits {
                    max_line_bytes: 0,
                    max_total_bytes: 4
                }
            )
            .is_err()
        );
    }
    #[test]
    fn accounting_property_across_buffer_sizes() {
        let input = b"1 INFO x ok\nbad\n2 ERROR y no\n\n";
        for capacity in 1..=32 {
            let r = analyze(
                BufReader::with_capacity(capacity, Cursor::new(input)),
                Limits::default(),
            )
            .unwrap();
            assert_eq!(r.lines, r.valid + r.malformed + r.oversized);
            assert_eq!(r.valid, r.info + r.warnings + r.errors);
            assert_eq!((r.lines, r.valid), (4, 2));
        }
    }
}
