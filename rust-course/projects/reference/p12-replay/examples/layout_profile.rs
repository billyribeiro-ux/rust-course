//! Bounded synthetic snapshot analytics: identical records, different layouts.
//! Run in release mode; stdout is one machine-readable measurement row.
use std::{hint::black_box, mem::size_of, process::ExitCode, time::Instant};

const MAX_ROWS: usize = 1_000_000;
const MAX_PASSES: usize = 200;

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Cold {
    timestamp: u64,
    account: u64,
    symbol: [u8; 16],
    audit_tag: [u8; 16],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Row {
    price: u64,
    quantity: u64,
    cold: Cold,
}

fn fixture(index: usize) -> Row {
    let i = index as u64;
    Row {
        price: 10_000 + i % 90_000,
        quantity: 1 + i % 100,
        cold: Cold {
            timestamp: 1_000_000 + i,
            account: i % 997,
            symbol: [(i % 26) as u8 + b'A'; 16],
            audit_tag: [(i % 251) as u8; 16],
        },
    }
}

fn reserved<T>(rows: usize) -> Result<Vec<T>, std::collections::TryReserveError> {
    let mut values = Vec::new();
    values.try_reserve_exact(rows)?;
    Ok(values)
}

enum Data {
    Aos(Vec<Row>),
    Soa {
        prices: Vec<u64>,
        quantities: Vec<u64>,
        cold: Vec<Cold>,
    },
}

impl Data {
    fn new(mode: &str, rows: usize) -> Result<Self, Box<dyn std::error::Error>> {
        match mode {
            "aos" => {
                let mut values = reserved(rows)?;
                values.extend((0..rows).map(fixture));
                Ok(Self::Aos(values))
            }
            "soa" => {
                let mut prices = reserved(rows)?;
                let mut quantities = reserved(rows)?;
                let mut cold = reserved(rows)?;
                for i in 0..rows {
                    let row = fixture(i);
                    prices.push(row.price);
                    quantities.push(row.quantity);
                    cold.push(row.cold);
                }
                Ok(Self::Soa {
                    prices,
                    quantities,
                    cold,
                })
            }
            _ => Err("layout must be aos or soa".into()),
        }
    }

    fn row(&self, index: usize) -> Row {
        match self {
            Self::Aos(values) => values[index],
            Self::Soa {
                prices,
                quantities,
                cold,
            } => Row {
                price: prices[index],
                quantity: quantities[index],
                cold: cold[index],
            },
        }
    }

    fn capacity_bytes(&self) -> usize {
        match self {
            Self::Aos(values) => values.capacity() * size_of::<Row>(),
            Self::Soa {
                prices,
                quantities,
                cold,
            } => {
                (prices.capacity() + quantities.capacity()) * size_of::<u64>()
                    + cold.capacity() * size_of::<Cold>()
            }
        }
    }

    fn allocation_requests(&self) -> usize {
        match self {
            Self::Aos(_) => 1,
            Self::Soa { .. } => 3,
        }
    }

    // Generator bounds prove this sum <= 1_000_000 * 99_999 * 100, below u64::MAX.
    // No unchecked external records enter this private fixture representation.
    fn notional(&self) -> u64 {
        match self {
            Self::Aos(values) => black_box(values.as_slice())
                .iter()
                .map(|row| row.price * row.quantity)
                .sum(),
            Self::Soa {
                prices, quantities, ..
            } => black_box(prices.as_slice())
                .iter()
                .zip(black_box(quantities.as_slice()))
                .map(|(price, quantity)| price * quantity)
                .sum(),
        }
    }
}

fn bounded_count(input: &str, maximum: usize) -> Result<usize, &'static str> {
    if input.len() > 20 {
        return Err("count text is too long");
    }
    match input.parse::<usize>() {
        Ok(value) if (1..=maximum).contains(&value) => Ok(value),
        _ => Err("count is invalid or outside the documented bound"),
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args_os()
        .skip(1)
        .map(|value| value.into_string().map_err(|_| "arguments must be UTF-8"))
        .collect::<Result<Vec<_>, _>>()?;
    let [mode, rows, passes] = args.as_slice() else {
        return Err("usage: layout_profile <aos|soa> <rows:1..1000000> <passes:1..200>".into());
    };
    let rows = bounded_count(rows, MAX_ROWS)?;
    let passes = bounded_count(passes, MAX_PASSES)?;
    if cfg!(debug_assertions) {
        return Err("comparative measurements require --release".into());
    }
    let begin = Instant::now();
    let data = Data::new(mode, rows)?;
    let generation_ns = begin.elapsed().as_nanos();
    let begin = Instant::now();
    for index in 0..rows {
        if data.row(index) != fixture(index) {
            return Err("full fixture comparison failed".into());
        }
    }
    let expected: u64 = (0..rows)
        .map(|i| {
            let row = fixture(i);
            row.price * row.quantity
        })
        .sum();
    let validation_ns = begin.elapsed().as_nanos();
    for _ in 0..2 {
        if black_box(data.notional()) != expected {
            return Err("warm-up result differs from fixture oracle".into());
        }
    }
    let begin = Instant::now();
    let mut checksum = 0u64;
    for _ in 0..passes {
        checksum += black_box(data.notional());
    }
    let scan_ns = begin.elapsed().as_nanos();
    if checksum != expected * passes as u64 {
        return Err("measured result differs from fixture oracle".into());
    }
    println!(
        "layout={mode}\trows={rows}\tpasses={passes}\trow_bytes={}\tcapacity_bytes={}\tallocation_requests={}\tgeneration_ns={generation_ns}\tvalidation_ns={validation_ns}\tscan_ns={scan_ns}\tchecksum={checksum}",
        size_of::<Row>(),
        data.capacity_bytes(),
        data.allocation_requests()
    );
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("layout profile: {error}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn layouts_preserve_every_field_and_analytics_for_boundary_sizes() {
        for count in [0, 1, 17, 99, 100, 101, 997, 1025] {
            let aos = Data::new("aos", count).unwrap();
            let soa = Data::new("soa", count).unwrap();
            for index in 0..count {
                assert_eq!(aos.row(index), soa.row(index));
            }
            assert_eq!(aos.notional(), soa.notional());
        }
        assert_eq!(Data::new("aos", 2).unwrap().notional(), 30_002);
        assert_eq!(fixture(0).cold.symbol, [b'A'; 16]);
        assert_eq!(fixture(26).cold.symbol, [b'A'; 16]);
    }
    #[test]
    fn limits_fail_before_allocating_and_modes_are_explicit() {
        for bad in ["0", "-1", "1000001", "NaN", "999999999999999999999"] {
            assert!(bounded_count(bad, MAX_ROWS).is_err());
        }
        assert_eq!(bounded_count("1000000", MAX_ROWS), Ok(MAX_ROWS));
        assert!(Data::new("typo", 10).is_err());
    }
}
