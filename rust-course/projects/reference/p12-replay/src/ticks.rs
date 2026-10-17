//! Bounded synthetic CSV tick ingestion with exact, deterministic aggregates.
//! This is a synchronous staged pipeline, not a concurrent execution claim.
use crate::Price;
use std::{
    collections::BTreeMap,
    error::Error,
    fmt,
    io::{self, BufRead, Read},
};

pub const HEADER: &str = "event_id,timestamp_ms,symbol,price,quantity";
pub const MAX_LINE_BYTES: usize = 256;
pub const MAX_EVENTS: usize = 1_000_000;
pub const MAX_SYMBOLS: usize = 64;
pub const MAX_BATCH: usize = 1024;

#[derive(Debug)]
pub enum TickError {
    Io(io::Error),
    InvalidConfig,
    Header,
    LineTooLong,
    InvalidRow(&'static str),
    Sequence { expected: u64, received: u64 },
    TimestampBackwards,
    EventLimit,
    SymbolLimit,
    Overflow,
    AtLine { line: usize, source: Box<TickError> },
}
impl fmt::Display for TickError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "input I/O failed: {error}"),
            Self::InvalidConfig => write!(
                f,
                "batch, event and symbol limits must be within supported nonzero bounds"
            ),
            Self::Header => write!(f, "expected exact CSV header: {HEADER}"),
            Self::LineTooLong => write!(
                f,
                "line exceeds {MAX_LINE_BYTES} bytes including its terminator"
            ),
            Self::InvalidRow(reason) => write!(f, "invalid tick: {reason}"),
            Self::Sequence { expected, received } => {
                write!(f, "expected event id {expected}, received {received}")
            }
            Self::TimestampBackwards => write!(f, "timestamp moved backwards"),
            Self::EventLimit => write!(f, "event budget exceeded"),
            Self::SymbolLimit => write!(f, "symbol budget exceeded"),
            Self::Overflow => write!(f, "aggregate overflow"),
            Self::AtLine { line, source } => write!(f, "line {line}: {source}"),
        }
    }
}
impl Error for TickError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::AtLine { source, .. } => Some(source),
            _ => None,
        }
    }
}
impl From<io::Error> for TickError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}
fn at_line(line: usize, source: TickError) -> TickError {
    TickError::AtLine {
        line,
        source: Box::new(source),
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub batch: usize,
    pub events: usize,
    pub symbols: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            batch: 64,
            events: MAX_EVENTS,
            symbols: MAX_SYMBOLS,
        }
    }
}
impl Limits {
    fn validate(self) -> Result<Self, TickError> {
        if !(1..=MAX_BATCH).contains(&self.batch)
            || !(1..=MAX_EVENTS).contains(&self.events)
            || !(1..=MAX_SYMBOLS).contains(&self.symbols)
        {
            return Err(TickError::InvalidConfig);
        }
        Ok(self)
    }
}

#[derive(Debug)]
struct Tick {
    id: u64,
    timestamp_ms: u64,
    symbol: String,
    price: Price,
    quantity: u64,
}

fn unsigned(text: &str) -> Result<u64, TickError> {
    if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
        return Err(TickError::InvalidRow(
            "integer fields require decimal digits only",
        ));
    }
    text.parse()
        .map_err(|_| TickError::InvalidRow("integer exceeds u64"))
}
fn parse(line: &str) -> Result<Tick, TickError> {
    let mut fields = line.split(',');
    let id = unsigned(
        fields
            .next()
            .ok_or(TickError::InvalidRow("missing event id"))?,
    )?;
    let timestamp_ms = unsigned(
        fields
            .next()
            .ok_or(TickError::InvalidRow("missing timestamp"))?,
    )?;
    let symbol = fields
        .next()
        .ok_or(TickError::InvalidRow("missing symbol"))?;
    if symbol.is_empty() || symbol.len() > 12 || !symbol.bytes().all(|b| b.is_ascii_uppercase()) {
        return Err(TickError::InvalidRow(
            "symbol requires 1..=12 ASCII uppercase letters",
        ));
    }
    let price = fields
        .next()
        .ok_or(TickError::InvalidRow("missing price"))?;
    if price.ends_with('.') {
        return Err(TickError::InvalidRow("fractional point requires digits"));
    }
    let price = Price::parse(price).map_err(|_| {
        TickError::InvalidRow(
            "price requires a positive decimal with at most four fractional digits",
        )
    })?;
    let quantity = unsigned(
        fields
            .next()
            .ok_or(TickError::InvalidRow("missing quantity"))?,
    )?;
    if !(1..=1_000_000).contains(&quantity) || fields.next().is_some() {
        return Err(TickError::InvalidRow(
            "quantity must be 1..=1000000 and exactly five fields are required",
        ));
    }
    Ok(Tick {
        id,
        timestamp_ms,
        symbol: symbol.to_owned(),
        price,
        quantity,
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Aggregate {
    pub count: u64,
    pub volume: u64,
    /// Exact sum of price ticks (1/10000 price unit) multiplied by quantity.
    pub notional_ticks: u128,
    pub first: Price,
    pub last: Price,
    pub low: Price,
    pub high: Price,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Summary {
    events: u64,
    first_timestamp_ms: Option<u64>,
    last_timestamp_ms: Option<u64>,
    symbols: BTreeMap<String, Aggregate>,
}
impl Summary {
    pub fn events(&self) -> u64 {
        self.events
    }
    pub fn first_timestamp_ms(&self) -> Option<u64> {
        self.first_timestamp_ms
    }
    pub fn last_timestamp_ms(&self) -> Option<u64> {
        self.last_timestamp_ms
    }
    pub fn symbols(&self) -> &BTreeMap<String, Aggregate> {
        &self.symbols
    }

    fn apply(&mut self, tick: Tick, limits: Limits) -> Result<(), TickError> {
        let next = self.events.checked_add(1).ok_or(TickError::Overflow)?;
        if tick.id != next {
            return Err(TickError::Sequence {
                expected: next,
                received: tick.id,
            });
        }
        if self.events >= limits.events as u64 {
            return Err(TickError::EventLimit);
        }
        if self
            .last_timestamp_ms
            .is_some_and(|last| tick.timestamp_ms < last)
        {
            return Err(TickError::TimestampBackwards);
        }
        let prior = self.symbols.get(&tick.symbol);
        if prior.is_none() && self.symbols.len() >= limits.symbols {
            return Err(TickError::SymbolLimit);
        }
        let notional = u128::from(tick.price.ticks())
            .checked_mul(u128::from(tick.quantity))
            .ok_or(TickError::Overflow)?;
        let aggregate = match prior {
            None => Aggregate {
                count: 1,
                volume: tick.quantity,
                notional_ticks: notional,
                first: tick.price,
                last: tick.price,
                low: tick.price,
                high: tick.price,
            },
            Some(prior) => Aggregate {
                count: prior.count.checked_add(1).ok_or(TickError::Overflow)?,
                volume: prior
                    .volume
                    .checked_add(tick.quantity)
                    .ok_or(TickError::Overflow)?,
                notional_ticks: prior
                    .notional_ticks
                    .checked_add(notional)
                    .ok_or(TickError::Overflow)?,
                first: prior.first,
                last: tick.price,
                low: if tick.price.ticks() < prior.low.ticks() {
                    tick.price
                } else {
                    prior.low
                },
                high: if tick.price.ticks() > prior.high.ticks() {
                    tick.price
                } else {
                    prior.high
                },
            },
        };
        // All fallible domain checks precede mutation. No partial aggregate commits.
        self.symbols.insert(tick.symbol, aggregate);
        self.events = next;
        self.first_timestamp_ms.get_or_insert(tick.timestamp_ms);
        self.last_timestamp_ms = Some(tick.timestamp_ms);
        Ok(())
    }
}

fn line(reader: &mut impl BufRead) -> Result<Option<String>, TickError> {
    let mut bytes = Vec::with_capacity(MAX_LINE_BYTES + 1);
    // take bounds allocation even if the untrusted input never supplies a newline.
    (&mut *reader)
        .take((MAX_LINE_BYTES + 1) as u64)
        .read_until(b'\n', &mut bytes)?;
    if bytes.is_empty() {
        return Ok(None);
    }
    if bytes.len() > MAX_LINE_BYTES {
        return Err(TickError::LineTooLong);
    }
    if bytes.last() == Some(&b'\n') {
        bytes.pop();
        if bytes.last() == Some(&b'\r') {
            bytes.pop();
        }
    }
    String::from_utf8(bytes)
        .map(Some)
        .map_err(|_| TickError::InvalidRow("line must be valid UTF-8"))
}

/// Consume one exact-header, bounded CSV stream. Returns no summary on any error.
/// Peak retained state is at most batch parsed ticks plus symbols aggregates and
/// one line retaining at most 257 input bytes, excluding buffering inside the
/// supplied reader. Allocator overhead/capacity is not an exact physical RAM claim.
pub fn replay(mut reader: impl BufRead, limits: Limits) -> Result<Summary, TickError> {
    let limits = limits.validate()?;
    if line(&mut reader)
        .map_err(|error| at_line(1, error))?
        .as_deref()
        != Some(HEADER)
    {
        return Err(at_line(1, TickError::Header));
    }
    let mut summary = Summary::default();
    let mut batch = Vec::with_capacity(limits.batch);
    let mut line_number = 1;
    let mut eof = false;
    while !eof {
        while batch.len() < limits.batch {
            line_number += 1;
            match line(&mut reader).map_err(|error| at_line(line_number, error))? {
                None => {
                    eof = true;
                    break;
                }
                Some(text) => {
                    if summary.events as usize + batch.len() >= limits.events {
                        return Err(at_line(line_number, TickError::EventLimit));
                    }
                    batch.push((
                        line_number,
                        parse(&text).map_err(|error| at_line(line_number, error))?,
                    ));
                }
            }
        }
        for (number, tick) in batch.drain(..) {
            summary
                .apply(tick, limits)
                .map_err(|error| at_line(number, error))?;
        }
    }
    Ok(summary)
}

#[cfg(test)]
mod tests {
    use super::*;
    const FIXTURE: &str = include_str!("../fixtures/ticks.csv");
    fn read(text: &str, limits: Limits) -> Result<Summary, TickError> {
        replay(text.as_bytes(), limits)
    }
    #[test]
    fn fixture_aggregates_and_batch_boundaries_are_exact() {
        let expected = read(
            FIXTURE,
            Limits {
                batch: 1,
                ..Limits::default()
            },
        )
        .unwrap();
        assert_eq!(expected.events(), 5);
        assert_eq!(expected.first_timestamp_ms(), Some(1_700_000_000_000));
        assert_eq!(expected.last_timestamp_ms(), Some(1_700_000_000_002));
        let a = &expected.symbols()["ALFA"];
        assert_eq!((a.count, a.volume, a.notional_ticks), (3, 7, 6_992_500));
        assert_eq!(
            (
                a.first.to_string(),
                a.last.to_string(),
                a.low.to_string(),
                a.high.to_string()
            ),
            (
                "100.0000".into(),
                "99.5000".into(),
                "99.5000".into(),
                "101.2500".into()
            )
        );
        assert_eq!(expected.symbols()["BETA"].notional_ticks, 2_505_000);
        for batch in [2, 3, 4, 5, 6, 1024] {
            assert_eq!(
                read(
                    FIXTURE,
                    Limits {
                        batch,
                        ..Limits::default()
                    }
                )
                .unwrap(),
                expected
            );
        }
    }
    #[test]
    fn rejected_id_timestamp_symbol_and_overflow_leave_state_unchanged() {
        let limits = Limits {
            symbols: 1,
            ..Limits::default()
        };
        let mut summary = Summary::default();
        summary
            .apply(parse("1,10,ALFA,1,2").unwrap(), limits)
            .unwrap();
        let original = summary.clone();
        for row in [
            "1,10,ALFA,1,2",
            "3,10,ALFA,1,2",
            "2,9,ALFA,1,2",
            "2,10,BETA,1,2",
        ] {
            assert!(summary.apply(parse(row).unwrap(), limits).is_err());
            assert_eq!(summary, original);
        }
        for field in ["count", "volume", "notional"] {
            let mut summary = original.clone();
            let a = summary.symbols.get_mut("ALFA").unwrap();
            match field {
                "count" => a.count = u64::MAX,
                "volume" => a.volume = u64::MAX,
                _ => a.notional_ticks = u128::MAX,
            }
            let before = summary.clone();
            assert!(matches!(
                summary.apply(parse("2,10,ALFA,1,2").unwrap(), limits),
                Err(TickError::Overflow)
            ));
            assert_eq!(summary, before);
        }
    }
    #[test]
    fn grammar_rejects_ambiguous_rows_and_accepts_exact_price_limit() {
        for row in [
            "",
            "1,0,ALFA,1,1,extra",
            "1,0,alfa,1,1",
            "1,0,A B,1,1",
            "1,0,ALFA,1.,1",
            "1,0,ALFA,1.00001,1",
            "1,0,ALFA,0,1",
            "1,0,ALFA,1,0",
            "1,0,ALFA,1,1000001",
            "1,+1,ALFA,1,1",
            "1,0,ALFA,1e2,1",
            "1,0,ALFA,1,18446744073709551616",
        ] {
            assert!(parse(row).is_err(), "{row}");
        }
        assert_eq!(
            parse("1,0,ALFA,100000000.0000,1000000")
                .unwrap()
                .price
                .ticks(),
            1_000_000_000_000
        );
        assert!(parse("1,0,ALFA,100000000.0001,1").is_err());
    }
    #[test]
    fn exact_header_empty_stream_crlf_and_unterminated_last_row() {
        assert!(read("", Limits::default()).is_err());
        assert!(read("wrong\n", Limits::default()).is_err());
        assert_eq!(read(HEADER, Limits::default()).unwrap().events(), 0);
        let input = format!("{HEADER}\r\n1,0,ALFA,1,1");
        assert_eq!(read(&input, Limits::default()).unwrap().events(), 1);
        assert!(read(&format!("{HEADER}\n\n"), Limits::default()).is_err());
    }
    #[test]
    fn limits_are_enforced_instead_of_returning_partial_success() {
        assert!(
            read(
                FIXTURE,
                Limits {
                    events: 4,
                    ..Limits::default()
                }
            )
            .is_err()
        );
        assert!(
            read(
                FIXTURE,
                Limits {
                    symbols: 1,
                    ..Limits::default()
                }
            )
            .is_err()
        );
        for limits in [
            Limits {
                batch: 0,
                ..Limits::default()
            },
            Limits {
                batch: 1025,
                ..Limits::default()
            },
            Limits {
                events: MAX_EVENTS + 1,
                ..Limits::default()
            },
            Limits {
                symbols: 65,
                ..Limits::default()
            },
        ] {
            assert!(matches!(
                read(FIXTURE, limits),
                Err(TickError::InvalidConfig)
            ));
        }
        let long = format!("{HEADER}\n{}", "1".repeat(MAX_LINE_BYTES + 1));
        assert!(matches!(
            read(&long, Limits::default()),
            Err(TickError::AtLine { line: 2, .. })
        ));
        let mut invalid = format!("{HEADER}\n").into_bytes();
        invalid.push(0xff);
        assert!(replay(invalid.as_slice(), Limits::default()).is_err());
    }
    #[test]
    fn large_lazy_input_does_not_require_retaining_the_file() {
        struct Generated {
            id: usize,
            buffer: std::io::Cursor<Vec<u8>>,
            peak: usize,
        }
        impl Read for Generated {
            fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
                if self.buffer.position() as usize == self.buffer.get_ref().len() {
                    if self.id == 20_001 {
                        return Ok(0);
                    }
                    let line = if self.id == 0 {
                        format!("{HEADER}\n")
                    } else {
                        format!("{},0,ALFA,1.2500,2\n", self.id)
                    };
                    self.id += 1;
                    self.peak = self.peak.max(line.len());
                    self.buffer = std::io::Cursor::new(line.into_bytes());
                }
                self.buffer.read(out)
            }
        }
        let mut generated = Generated {
            id: 0,
            buffer: std::io::Cursor::new(Vec::new()),
            peak: 0,
        };
        let summary = replay(
            io::BufReader::with_capacity(128, &mut generated),
            Limits {
                batch: 3,
                ..Limits::default()
            },
        )
        .unwrap();
        assert_eq!(summary.events(), 20_000);
        assert_eq!(summary.symbols()["ALFA"].volume, 40_000);
        assert_eq!(summary.symbols()["ALFA"].notional_ticks, 500_000_000);
        assert!(generated.peak <= MAX_LINE_BYTES);
    }
    #[test]
    fn input_errors_propagate_and_unterminated_stream_reads_are_bounded() {
        struct Failure;
        impl Read for Failure {
            fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
                Err(io::Error::other("injected"))
            }
        }
        assert!(replay(io::BufReader::new(Failure), Limits::default()).is_err());
        let mut endless = io::BufReader::with_capacity(16, io::repeat(b'x'));
        assert!(matches!(line(&mut endless), Err(TickError::LineTooLong)));
    }
}
