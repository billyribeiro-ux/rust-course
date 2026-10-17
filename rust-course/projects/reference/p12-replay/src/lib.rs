//! Deterministic synthetic resting-order replay; no trading or execution model.
use std::{collections::BTreeMap, fmt};

pub mod ticks;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct OrderId(pub u64);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EventId(pub u64);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Price(u64);
impl Price {
    pub const SCALE: u64 = 10_000;
    pub fn from_ticks(ticks: u64) -> Result<Self, Error> {
        if ticks == 0 || ticks > 1_000_000_000_000 {
            Err(Error::InvalidPrice)
        } else {
            Ok(Self(ticks))
        }
    }
    pub fn ticks(self) -> u64 {
        self.0
    }
    pub fn parse(input: &str) -> Result<Self, Error> {
        let (whole, fractional) = input.split_once('.').unwrap_or((input, ""));
        if whole.is_empty()
            || !whole.bytes().all(|b| b.is_ascii_digit())
            || fractional.len() > 4
            || !fractional.bytes().all(|b| b.is_ascii_digit())
        {
            return Err(Error::InvalidPrice);
        }
        let whole = whole.parse::<u64>().map_err(|_| Error::InvalidPrice)?;
        let mut fraction = if fractional.is_empty() {
            0
        } else {
            fractional.parse::<u64>().map_err(|_| Error::InvalidPrice)?
        };
        for _ in fractional.len()..4 {
            fraction *= 10;
        }
        Self::from_ticks(
            whole
                .checked_mul(Self::SCALE)
                .and_then(|n| n.checked_add(fraction))
                .ok_or(Error::InvalidPrice)?,
        )
    }
}
impl fmt::Display for Price {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{:04}", self.0 / Self::SCALE, self.0 % Self::SCALE)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Buy,
    Sell,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Order {
    pub side: Side,
    pub price: Price,
    pub quantity: u64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    Place { id: OrderId, order: Order },
    Cancel { id: OrderId },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Event {
    pub sequence: EventId,
    pub action: Action,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    InvalidPrice,
    InvalidQuantity,
    InvalidLimit,
    Sequence { expected: u64, received: u64 },
    Duplicate(OrderId),
    Missing(OrderId),
    Capacity,
    Overflow,
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for Error {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Replay {
    orders: BTreeMap<OrderId, Order>,
    next_sequence: u64,
    capacity: usize,
    accepted: u64,
    /// Sum of submitted price ticks * quantity, not profit or traded volume.
    submitted_notional_ticks: u128,
}
impl Replay {
    pub fn new(capacity: usize) -> Result<Self, Error> {
        if capacity == 0 || capacity > 4096 {
            return Err(Error::InvalidLimit);
        }
        Ok(Self {
            orders: BTreeMap::new(),
            next_sequence: 1,
            capacity,
            accepted: 0,
            submitted_notional_ticks: 0,
        })
    }
    pub fn orders(&self) -> &BTreeMap<OrderId, Order> {
        &self.orders
    }
    pub fn accepted(&self) -> u64 {
        self.accepted
    }
    pub fn submitted_notional_ticks(&self) -> u128 {
        self.submitted_notional_ticks
    }
    /// Validation precedes mutation: a rejected event leaves the entire state unchanged.
    pub fn apply(&mut self, event: Event) -> Result<(), Error> {
        if event.sequence.0 != self.next_sequence {
            return Err(Error::Sequence {
                expected: self.next_sequence,
                received: event.sequence.0,
            });
        }
        let next = self.next_sequence.checked_add(1).ok_or(Error::Overflow)?;
        let accepted = self.accepted.checked_add(1).ok_or(Error::Overflow)?;
        match event.action {
            Action::Place { id, order } => {
                if order.quantity == 0 || order.quantity > 1_000_000_000 {
                    return Err(Error::InvalidQuantity);
                }
                if self.orders.contains_key(&id) {
                    return Err(Error::Duplicate(id));
                }
                if self.orders.len() == self.capacity {
                    return Err(Error::Capacity);
                }
                let notional = self
                    .submitted_notional_ticks
                    .checked_add(u128::from(order.price.0) * u128::from(order.quantity))
                    .ok_or(Error::Overflow)?;
                self.orders.insert(id, order);
                self.submitted_notional_ticks = notional;
            }
            Action::Cancel { id } => {
                if self.orders.remove(&id).is_none() {
                    return Err(Error::Missing(id));
                }
            }
        }
        self.next_sequence = next;
        self.accepted = accepted;
        Ok(())
    }
    /// Stable quick check, not a collision-free substitute for comparing complete states.
    pub fn digest(&self) -> u64 {
        let mut hash = 0xcbf2_9ce4_8422_2325u64;
        let mut feed = |number: u64| {
            for b in number.to_le_bytes() {
                hash = (hash ^ u64::from(b)).wrapping_mul(0x100_0000_01b3);
            }
        };
        feed(self.accepted);
        feed(self.submitted_notional_ticks as u64);
        feed((self.submitted_notional_ticks >> 64) as u64);
        for (id, order) in &self.orders {
            feed(id.0);
            feed(u64::from(order.side == Side::Sell));
            feed(order.price.0);
            feed(order.quantity);
        }
        hash
    }
}

pub fn synthetic(count: usize) -> Result<impl Iterator<Item = Event>, Error> {
    if count > 10_000_000 {
        return Err(Error::InvalidLimit);
    }
    Ok((0..count).map(|index| {
        let group = (index / 4) as u64;
        let first = group * 2 + 1;
        let action = match index % 4 {
            0 => Action::Place {
                id: OrderId(first),
                order: Order {
                    side: Side::Buy,
                    price: Price(1_000_000 + group % 97),
                    quantity: 10,
                },
            },
            1 => Action::Place {
                id: OrderId(first + 1),
                order: Order {
                    side: Side::Sell,
                    price: Price(1_000_100 + group % 97),
                    quantity: 12,
                },
            },
            2 => Action::Cancel { id: OrderId(first) },
            _ => Action::Cancel {
                id: OrderId(first + 1),
            },
        };
        Event {
            sequence: EventId(index as u64 + 1),
            action,
        }
    }))
}
pub fn run(count: usize) -> Result<Replay, Error> {
    let mut replay = Replay::new(4096)?;
    for event in synthetic(count)? {
        replay.apply(event)?;
    }
    Ok(replay)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn place(sequence: u64, id: u64) -> Event {
        Event {
            sequence: EventId(sequence),
            action: Action::Place {
                id: OrderId(id),
                order: Order {
                    side: Side::Buy,
                    price: Price::parse("12.3400").unwrap(),
                    quantity: 3,
                },
            },
        }
    }
    #[test]
    fn exact_decimal_conversion_rejects_bad_inputs() {
        assert_eq!(Price::parse("0.0001").unwrap().ticks(), 1);
        assert_eq!(Price::parse("12.34").unwrap().to_string(), "12.3400");
        for bad in [
            "NaN",
            "-1",
            "1e3",
            "1.00001",
            "1.2.3",
            "0",
            "9999999999999999999999",
        ] {
            assert!(Price::parse(bad).is_err(), "{bad}");
        }
    }
    #[test]
    fn replay_is_identical_including_full_state() {
        assert_eq!(run(10_003).unwrap(), run(10_003).unwrap());
    }
    #[test]
    fn rejection_is_transactional_and_corrected_event_can_retry() {
        let mut r = Replay::new(3).unwrap();
        r.apply(place(1, 7)).unwrap();
        let before = r.clone();
        assert_eq!(r.apply(place(2, 7)), Err(Error::Duplicate(OrderId(7))));
        assert_eq!(r, before);
        assert!(matches!(r.apply(place(4, 8)), Err(Error::Sequence { .. })));
        assert_eq!(r, before);
        r.apply(place(2, 8)).unwrap();
    }
    #[test]
    fn capacity_and_missing_cancel_preserve_state() {
        let mut r = Replay::new(1).unwrap();
        r.apply(place(1, 1)).unwrap();
        let before = r.clone();
        assert_eq!(r.apply(place(2, 2)), Err(Error::Capacity));
        assert_eq!(r, before);
        assert_eq!(
            r.apply(Event {
                sequence: EventId(2),
                action: Action::Cancel { id: OrderId(99) }
            }),
            Err(Error::Missing(OrderId(99)))
        );
        assert_eq!(r, before);
        r.apply(Event {
            sequence: EventId(2),
            action: Action::Cancel { id: OrderId(1) },
        })
        .unwrap();
        r.apply(place(3, 2)).unwrap();
    }
    #[test]
    fn injected_accounting_overflow_does_not_partially_insert() {
        let mut r = Replay::new(2).unwrap();
        r.submitted_notional_ticks = u128::MAX;
        let before = r.clone();
        assert_eq!(r.apply(place(1, 1)), Err(Error::Overflow));
        assert_eq!(r, before);
    }
    #[test]
    fn injected_counter_overflow_preserves_complete_state() {
        for overflow_sequence in [true, false] {
            let mut r = Replay::new(2).unwrap();
            r.apply(place(1, 7)).unwrap();
            if overflow_sequence {
                r.next_sequence = u64::MAX;
            } else {
                // This deliberately injected state exercises the defensive counter
                // check; public construction keeps accepted and sequence related.
                r.accepted = u64::MAX;
            }
            let before = r.clone();
            for event in [
                place(r.next_sequence, 8),
                Event {
                    sequence: EventId(r.next_sequence),
                    action: Action::Cancel { id: OrderId(7) },
                },
            ] {
                assert_eq!(r.apply(event), Err(Error::Overflow));
                assert_eq!(r, before);
            }
        }
    }
    #[test]
    fn synthetic_processing_keeps_live_state_bounded() {
        let mut r = Replay::new(2).unwrap();
        for event in synthetic(4000).unwrap() {
            r.apply(event).unwrap();
            assert!(r.orders.len() <= 2);
        }
        assert!(r.orders.is_empty());
    }
}
