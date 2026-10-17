//! A borrowed parser and two dispatch choices, with no speed claim.
use std::fmt;

const MAX_BYTES: usize = 4096;
const MAX_ROWS: usize = 64;

#[derive(Debug, PartialEq, Eq)]
struct Tick<'a> {
    symbol: &'a str,
    price: u64,
    quantity: u64,
}

#[derive(Debug, PartialEq, Eq)]
struct OwnedTick {
    symbol: String,
    price: u64,
    quantity: u64,
}

#[derive(Debug, PartialEq, Eq)]
enum ParseError {
    InputLimit,
    RowLimit,
    Fields,
    Symbol,
    Number,
    Range,
}
impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InputLimit => "input exceeds 4096 bytes",
            Self::RowLimit => "input exceeds 64 rows",
            Self::Fields => "each row must have symbol,price,quantity",
            Self::Symbol => "symbol must contain 1 through 8 uppercase ASCII letters",
            Self::Number => "price and quantity must be unsigned decimal integers",
            Self::Range => "price must be 1..=1000000000000; quantity 1..=1000000000",
        })
    }
}
impl std::error::Error for ParseError {}

fn parse(input: &str) -> Result<Vec<Tick<'_>>, ParseError> {
    if input.len() > MAX_BYTES {
        return Err(ParseError::InputLimit);
    }
    let mut rows = Vec::new();
    // The entire borrowed input is already bounded; this is not unbounded I/O.
    for line in input.lines() {
        if rows.len() == MAX_ROWS {
            return Err(ParseError::RowLimit);
        }
        let mut fields = line.split(',');
        let symbol = fields.next().ok_or(ParseError::Fields)?;
        let price = fields.next().ok_or(ParseError::Fields)?;
        let quantity = fields.next().ok_or(ParseError::Fields)?;
        if fields.next().is_some() {
            return Err(ParseError::Fields);
        }
        if symbol.is_empty() || symbol.len() > 8 || !symbol.bytes().all(|b| b.is_ascii_uppercase())
        {
            return Err(ParseError::Symbol);
        }
        if price.is_empty()
            || quantity.is_empty()
            || !price.bytes().all(|b| b.is_ascii_digit())
            || !quantity.bytes().all(|b| b.is_ascii_digit())
        {
            return Err(ParseError::Number);
        }
        let price: u64 = price.parse().map_err(|_| ParseError::Number)?;
        let quantity: u64 = quantity.parse().map_err(|_| ParseError::Number)?;
        if !(1..=1_000_000_000_000).contains(&price) || !(1..=1_000_000_000).contains(&quantity) {
            return Err(ParseError::Range);
        }
        rows.push(Tick {
            symbol,
            price,
            quantity,
        });
    }
    Ok(rows)
}

fn retain(rows: &[Tick<'_>]) -> Vec<OwnedTick> {
    rows.iter()
        .map(|row| OwnedTick {
            symbol: row.symbol.to_owned(),
            price: row.price,
            quantity: row.quantity,
        })
        .collect()
}

trait Metric {
    fn score(&self, tick: &Tick<'_>) -> u128;
}
struct Notional;
impl Metric for Notional {
    fn score(&self, tick: &Tick<'_>) -> u128 {
        u128::from(tick.price) * u128::from(tick.quantity)
    }
}
struct Quantity;
impl Metric for Quantity {
    fn score(&self, tick: &Tick<'_>) -> u128 {
        u128::from(tick.quantity)
    }
}

fn total_static<M: Metric>(rows: &[Tick<'_>], metric: &M) -> u128 {
    rows.iter().map(|row| metric.score(row)).sum()
}
fn total_dynamic(rows: &[Tick<'_>], metric: &dyn Metric) -> u128 {
    rows.iter().map(|row| metric.score(row)).sum()
}

fn main() -> Result<(), ParseError> {
    let input = String::from("ABC,1200,3\nXYZ,2050,2\n");
    let rows = parse(&input)?;
    let static_total = total_static(&rows, &Notional);
    // These trait objects borrow two stack-resident values; dyn does not require Box.
    let metrics: [&dyn Metric; 2] = [&Notional, &Quantity];
    let totals = metrics.map(|metric| total_dynamic(&rows, metric));
    println!(
        "rows={} static_notional={static_total} dynamic_totals={totals:?}",
        rows.len()
    );
    let owned = retain(&rows);
    drop(rows);
    drop(input);
    println!(
        "retained {} owned rows after dropping the source buffer",
        owned.len()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn borrowed_fields_alias_input_and_owned_fields_survive_it() {
        let owned = {
            let input = String::from("ABC,1200,3\nXYZ,2050,2\n");
            let rows = parse(&input).unwrap();
            assert!(std::ptr::eq(rows[0].symbol.as_ptr(), input.as_ptr()));
            assert_eq!(total_static(&rows, &Notional), 7700);
            assert_eq!(total_dynamic(&rows, &Notional), 7700);
            assert_eq!(total_dynamic(&rows, &Quantity), 5);
            retain(&rows)
        };
        assert_eq!(
            owned,
            [
                OwnedTick {
                    symbol: "ABC".into(),
                    price: 1200,
                    quantity: 3
                },
                OwnedTick {
                    symbol: "XYZ".into(),
                    price: 2050,
                    quantity: 2
                }
            ]
        );
    }
    #[test]
    fn invalid_input_and_resource_bounds_are_explicit() {
        for input in [
            "abc,1,1",
            "ABCDEFGHI,1,1",
            "A,0,1",
            "A,1,0",
            "A,1,-2",
            "A,1,2,3",
            "A,1",
            "A,18446744073709551616,1",
        ] {
            assert!(parse(input).is_err(), "{input}");
        }
        assert_eq!(parse(&"A,1,1\n".repeat(65)), Err(ParseError::RowLimit));
        assert_eq!(parse(&"x".repeat(4097)), Err(ParseError::InputLimit));
        assert!(parse("").unwrap().is_empty());
        assert_eq!(parse(&"A,1,1\n".repeat(64)).unwrap().len(), 64);
    }
}
