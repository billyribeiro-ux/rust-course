//! Synthetic paper trading only. No exchange, broker, credentials, or real execution.
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, VecDeque},
    fs::{File, OpenOptions},
    io::{self, BufRead, BufReader, Read, Write},
    path::Path,
};
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Tick {
    pub event_id: String,
    pub symbol: String,
    pub price_cents: i64,
    pub sequence: u64,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    Buy,
    Sell,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Order {
    pub order_id: String,
    pub symbol: String,
    pub side: Side,
    pub quantity: i64,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Fill {
    pub order_id: String,
    pub symbol: String,
    pub side: Side,
    pub quantity: i64,
    pub price_cents: i64,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub enum Event {
    Tick(Tick),
    Order(Order),
    Watchlist(Vec<String>),
    ScheduleReplay(replay::ReplaySpec),
    ReplayStep { job_id: String, index: usize },
    CancelReplay { job_id: String },
    FailReplay { job_id: String, reason: String },
}
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Record {
    version: u8,
    #[serde(default)]
    position: Option<u64>,
    event: Event,
}
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Quote {
    pub symbol: String,
    pub price_cents: i64,
    pub sequence: u64,
}
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Position {
    pub symbol: String,
    pub quantity: i64,
}
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct Snapshot {
    pub simulated: bool,
    pub cash_cents: i64,
    pub quotes: Vec<Quote>,
    pub positions: Vec<Position>,
    pub orders: Vec<Fill>,
    pub watchlist: Vec<String>,
    pub events: u64,
}
#[derive(Debug)]
pub struct Engine {
    cash: i64,
    quotes: BTreeMap<String, Quote>,
    positions: BTreeMap<String, i64>,
    fills: BTreeMap<String, Fill>,
    recent_orders: VecDeque<String>,
    seen: BTreeMap<String, Tick>,
    watchlist: Vec<String>,
    events: u64,
    history: VecDeque<replay::ChartPoint>,
    jobs: BTreeMap<String, replay::ReplayJob>,
}
impl Default for Engine {
    fn default() -> Self {
        Self {
            cash: 1_000_000,
            quotes: BTreeMap::new(),
            positions: BTreeMap::new(),
            fills: BTreeMap::new(),
            recent_orders: VecDeque::new(),
            seen: BTreeMap::new(),
            watchlist: vec!["RUST".into()],
            events: 0,
            history: VecDeque::new(),
            jobs: BTreeMap::new(),
        }
    }
}
fn symbol(s: &str) -> bool {
    !s.is_empty() && s.len() <= 8 && s.bytes().all(|b| b.is_ascii_uppercase())
}
fn identifier(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 80
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_".contains(&b))
}
impl Engine {
    fn validate_tick(&self, t: &Tick) -> Result<bool, &'static str> {
        if let Some(old) = self.seen.get(&t.event_id) {
            return if old == t {
                Ok(false)
            } else {
                Err("event id conflict")
            };
        }
        if !identifier(&t.event_id)
            || !symbol(&t.symbol)
            || t.price_cents <= 0
            || t.price_cents > 100_000_000
        {
            return Err("invalid tick");
        }
        if self
            .quotes
            .get(&t.symbol)
            .is_some_and(|q| t.sequence <= q.sequence)
        {
            return Err("out-of-order tick");
        }
        if !self.quotes.contains_key(&t.symbol) && self.quotes.len() >= 32 {
            return Err("symbol budget");
        }
        Ok(true)
    }
    /// Validation is read-only. `false` denotes an exact retry already applied.
    pub fn validate(&self, event: &Event) -> Result<bool, &'static str> {
        match event {
            Event::Tick(t) => {
                if !self.validate_tick(t)? {
                    return Ok(false);
                }
            }
            Event::Order(o) => {
                if let Some(fill) = self.fills.get(&o.order_id) {
                    return if fill.symbol == o.symbol
                        && fill.quantity == o.quantity
                        && fill.side == o.side
                    {
                        Ok(false)
                    } else {
                        Err("order id conflict")
                    };
                }
                if !identifier(&o.order_id)
                    || !symbol(&o.symbol)
                    || !(1..=10_000).contains(&o.quantity)
                {
                    return Err("invalid order");
                }
                let quote = self.quotes.get(&o.symbol).ok_or("quote required")?;
                let cost = quote
                    .price_cents
                    .checked_mul(o.quantity)
                    .ok_or("cost overflow")?;
                let held = *self.positions.get(&o.symbol).unwrap_or(&0);
                match o.side {
                    Side::Buy => {
                        if cost > self.cash {
                            return Err("insufficient simulated cash");
                        }
                        held.checked_add(o.quantity).ok_or("position overflow")?;
                    }
                    Side::Sell => {
                        if held < o.quantity {
                            return Err("insufficient position");
                        }
                        self.cash.checked_add(cost).ok_or("cash overflow")?;
                    }
                }
            }
            Event::ScheduleReplay(spec) => {
                if let Some(job) = self.jobs.get(&spec.job_id) {
                    return if job.spec == *spec {
                        Ok(false)
                    } else {
                        Err("job id conflict")
                    };
                }
                if !identifier(&spec.job_id)
                    || spec.ticks.is_empty()
                    || spec.ticks.len() > 64
                    || self.jobs.len() >= 16
                {
                    return Err("replay job budget or identity");
                }
                for tick in &spec.ticks {
                    if !identifier(&tick.event_id)
                        || !symbol(&tick.symbol)
                        || tick.price_cents <= 0
                        || tick.price_cents > 100_000_000
                    {
                        return Err("invalid replay tick");
                    }
                }
                // Reserve enough event slots for already queued work plus this job and cancellation.
                let pending: usize = self
                    .jobs
                    .values()
                    .filter(|j| j.active())
                    .map(|j| j.spec.ticks.len() - j.next)
                    .sum();
                if self.events + (pending + spec.ticks.len() + 32) as u64 >= 10_000 {
                    return Err("insufficient replay event budget");
                }
            }
            Event::ReplayStep { job_id, index } => {
                let job = self.jobs.get(job_id).ok_or("job not found")?;
                if *index < job.next {
                    return Ok(false);
                }
                if !job.active() || *index != job.next {
                    return Err("job no longer runnable or cursor conflict");
                }
                let tick = &job.spec.ticks[*index];
                self.validate_tick(tick)?;
            }
            Event::CancelReplay { job_id } => {
                let job = self.jobs.get(job_id).ok_or("job not found")?;
                if job.status == replay::ReplayStatus::Cancelled {
                    return Ok(false);
                }
                if !job.active() {
                    return Err("job already terminal");
                }
            }
            Event::FailReplay { job_id, reason } => {
                if reason.len() > 100 {
                    return Err("failure reason budget");
                }
                if !self.jobs.get(job_id).is_some_and(|j| j.active()) {
                    return Err("job no longer runnable");
                }
            }
            Event::Watchlist(symbols) => {
                if symbols.len() > 32 || symbols.iter().any(|s| !symbol(s)) {
                    return Err("invalid watchlist");
                }
                let unique: std::collections::BTreeSet<_> = symbols.iter().collect();
                if unique.len() != symbols.len() {
                    return Err("duplicate watchlist symbol");
                }
            }
        }
        if self.events >= 10_000 {
            return Err("event budget reached; archive session");
        }
        if matches!(
            event,
            Event::Tick(_) | Event::Order(_) | Event::Watchlist(_)
        ) {
            let reserved: usize = self
                .jobs
                .values()
                .filter(|job| job.active())
                .map(|job| job.spec.ticks.len() - job.next)
                .sum();
            if self.events + reserved as u64 >= 10_000 {
                return Err("remaining event capacity reserved for accepted replay jobs");
            }
        }
        Ok(true)
    }
    pub fn apply(&mut self, event: Event) -> Result<bool, &'static str> {
        if !self.validate(&event)? {
            return Ok(false);
        }
        match event {
            Event::Tick(t) => self.accept_tick(t),
            Event::Order(o) => {
                let price = self.quotes[&o.symbol].price_cents;
                let cost = price * o.quantity;
                let held = self.positions.entry(o.symbol.clone()).or_default();
                match o.side {
                    Side::Buy => {
                        self.cash -= cost;
                        *held += o.quantity;
                    }
                    Side::Sell => {
                        self.cash += cost;
                        *held -= o.quantity;
                    }
                }
                let accepted_id = o.order_id.clone();
                self.fills.insert(
                    o.order_id.clone(),
                    Fill {
                        order_id: o.order_id,
                        symbol: o.symbol,
                        side: o.side,
                        quantity: o.quantity,
                        price_cents: price,
                    },
                );
                self.recent_orders.push_back(accepted_id);
                if self.recent_orders.len() > 100 {
                    self.recent_orders.pop_front();
                }
            }
            Event::Watchlist(symbols) => self.watchlist = symbols,
            Event::ScheduleReplay(spec) => {
                self.jobs.insert(
                    spec.job_id.clone(),
                    replay::ReplayJob {
                        spec,
                        next: 0,
                        status: replay::ReplayStatus::Queued,
                        error: None,
                    },
                );
            }
            Event::ReplayStep { job_id, index } => {
                let tick = self.jobs[&job_id].spec.ticks[index].clone();
                if self.validate_tick(&tick)? {
                    self.accept_tick(tick);
                }
                let job = self
                    .jobs
                    .get_mut(&job_id)
                    .expect("validated job exists under exclusive ownership");
                job.next += 1;
                job.status = if job.next == job.spec.ticks.len() {
                    replay::ReplayStatus::Completed
                } else {
                    replay::ReplayStatus::Running
                };
            }
            Event::CancelReplay { job_id } => {
                self.jobs
                    .get_mut(&job_id)
                    .expect("validated job exists")
                    .status = replay::ReplayStatus::Cancelled;
            }
            Event::FailReplay { job_id, reason } => {
                let job = self.jobs.get_mut(&job_id).expect("validated job exists");
                job.status = replay::ReplayStatus::Failed;
                job.error = Some(reason);
            }
        }
        self.events += 1;
        Ok(true)
    }
    fn accept_tick(&mut self, t: Tick) {
        self.history.push_back(replay::ChartPoint {
            event_position: self.events + 1,
            symbol: t.symbol.clone(),
            price_cents: t.price_cents,
            sequence: t.sequence,
        });
        if self.history.len() > 1024 {
            self.history.pop_front();
        }
        self.quotes.insert(
            t.symbol.clone(),
            Quote {
                symbol: t.symbol.clone(),
                price_cents: t.price_cents,
                sequence: t.sequence,
            },
        );
        self.seen.insert(t.event_id.clone(), t);
    }
    pub fn fill(&self, id: &str) -> Option<&Fill> {
        self.fills.get(id)
    }
    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            simulated: true,
            cash_cents: self.cash,
            quotes: self.quotes.values().cloned().collect(),
            positions: self
                .positions
                .iter()
                .map(|(s, q)| Position {
                    symbol: s.clone(),
                    quantity: *q,
                })
                .collect(),
            orders: self
                .recent_orders
                .iter()
                .rev()
                .filter_map(|id| self.fills.get(id))
                .cloned()
                .collect(),
            watchlist: self.watchlist.clone(),
            events: self.events,
        }
    }
}
/// Distinguishes a rejected command from identity conflicts and uncertain storage outcomes.
#[derive(Debug)]
pub enum CommitError {
    Rejected(&'static str),
    Conflict(&'static str),
    Storage(String),
}
impl std::fmt::Display for CommitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Rejected(reason) | Self::Conflict(reason) => f.write_str(reason),
            Self::Storage(reason) => f.write_str(reason),
        }
    }
}
impl std::error::Error for CommitError {}
impl CommitError {
    fn validation(reason: &'static str) -> Self {
        match reason {
            "event id conflict" | "order id conflict" | "job id conflict" => Self::Conflict(reason),
            _ => Self::Rejected(reason),
        }
    }
}
/// The authoritative engine cannot be mutated without journal persistence by callers.
///
/// ```compile_fail,E0616
/// use p16_workbench::{Store, Event};
/// fn bypass_persistence(store: &mut Store) {
///     store.engine.apply(Event::Watchlist(vec![]));
/// }
/// ```
pub struct Store {
    pub(crate) engine: Engine,
    file: File,
    poisoned: bool,
    records: u64,
}
impl Store {
    /// A single process owns this file. No cross-process locking; do not start two writers.
    /// sync_data precedes success. Creation-directory durability and torn-record repair are not promised.
    pub fn open(path: &Path) -> io::Result<Self> {
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .read(true)
            .open(path)?;
        let mut engine = Engine::default();
        let mut reader = BufReader::new(file.try_clone()?);
        let mut line = Vec::new();
        let mut records = 0;
        loop {
            line.clear();
            let read = reader.by_ref().take(16385).read_until(b'\n', &mut line)?;
            if read == 0 {
                break;
            }
            records += 1;
            if records > 10_000 {
                return Err(io::Error::new(io::ErrorKind::InvalidData, "record budget"));
            }
            if read > 16384 || line.last() != Some(&b'\n') {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "oversized or incomplete record; preserve file for recovery",
                ));
            }
            let record: Record = serde_json::from_slice(&line)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
            if !matches!(record.version, 1 | 2)
                || (record.version == 2 && record.position != Some(records))
                || (record.version == 1 && record.position.is_some())
            {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "unknown record version or non-contiguous journal position",
                ));
            }
            let applied = engine
                .apply(record.event)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
            if !applied {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "duplicate logical command in journal",
                ));
            }
        }
        Ok(Self {
            engine,
            file,
            poisoned: false,
            records,
        })
    }
    pub fn snapshot(&self) -> Snapshot {
        self.engine.snapshot()
    }
    pub fn commit(&mut self, event: Event) -> Result<bool, CommitError> {
        if self.poisoned {
            return Err(CommitError::Storage(
                "storage uncertain; stop and inspect journal".into(),
            ));
        }
        if !self
            .engine
            .validate(&event)
            .map_err(CommitError::validation)?
        {
            return Ok(false);
        }
        if self.records >= 10_000 {
            return Err(CommitError::Rejected("physical journal record budget"));
        }
        let mut bytes = serde_json::to_vec(&Record {
            version: 2,
            position: Some(self.records + 1),
            event: event.clone(),
        })
        .map_err(|e| CommitError::Storage(format!("encoding failed before append: {e}")))?;
        bytes.push(b'\n');
        if bytes.len() > 16384 {
            return Err(CommitError::Rejected("journal record budget"));
        }
        if let Err(error) = self
            .file
            .write_all(&bytes)
            .and_then(|()| self.file.sync_data())
        {
            self.poisoned = true;
            return Err(CommitError::Storage(format!("storage uncertain: {error}")));
        }
        self.records += 1;
        match self.engine.apply(event) {
            Ok(true) => Ok(true),
            result => {
                self.poisoned = true;
                Err(CommitError::Storage(format!(
                    "durable record could not be applied consistently: {result:?}"
                )))
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn tick(id: &str, seq: u64) -> Event {
        Event::Tick(Tick {
            event_id: id.into(),
            symbol: "RUST".into(),
            price_cents: 125,
            sequence: seq,
        })
    }
    fn buy(id: &str, n: i64) -> Event {
        Event::Order(Order {
            order_id: id.into(),
            symbol: "RUST".into(),
            side: Side::Buy,
            quantity: n,
        })
    }
    #[test]
    fn duplicate_out_of_order_and_conflicts() {
        let mut e = Engine::default();
        assert!(e.apply(tick("t1", 1)).unwrap());
        assert!(!e.apply(tick("t1", 1)).unwrap());
        assert!(e.apply(tick("t1", 2)).is_err());
        assert!(e.apply(tick("old", 0)).is_err());
        e.apply(buy("o1", 3)).unwrap();
        assert!(!e.apply(buy("o1", 3)).unwrap());
        assert!(e.apply(buy("o1", 4)).is_err());
        assert_eq!(e.snapshot().cash_cents, 999625);
        assert_eq!(e.snapshot().positions[0].quantity, 3);
    }
    #[test]
    fn rejected_order_is_atomic() {
        let mut e = Engine::default();
        e.apply(tick("t", 1)).unwrap();
        let before = e.snapshot();
        assert!(e.apply(buy("huge", 10000)).is_err());
        assert_eq!(before, e.snapshot());
    }
    #[test]
    fn buy_sell_conserves_cash_without_fees() {
        for quantity in 1..500 {
            let mut e = Engine::default();
            e.apply(tick("t", 1)).unwrap();
            e.apply(buy("b", quantity)).unwrap();
            e.apply(Event::Order(Order {
                order_id: "s".into(),
                symbol: "RUST".into(),
                side: Side::Sell,
                quantity,
            }))
            .unwrap();
            assert_eq!(e.snapshot().cash_cents, 1_000_000);
            assert_eq!(e.snapshot().positions[0].quantity, 0);
        }
    }
    #[test]
    fn restart_replays_and_truncated_tail_fails() {
        let path = std::env::temp_dir().join(format!(
            "course-workbench-{}-{}.jsonl",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let expected = {
            let mut s = Store::open(&path).unwrap();
            s.commit(tick("t", 1)).unwrap();
            s.commit(buy("b", 5)).unwrap();
            s.engine.snapshot()
        };
        assert_eq!(Store::open(&path).unwrap().engine.snapshot(), expected);
        let mut f = OpenOptions::new().append(true).open(&path).unwrap();
        f.write_all(b"{incomplete").unwrap();
        drop(f);
        assert!(Store::open(&path).is_err());
        std::fs::remove_file(path).unwrap();
    }
}

pub mod http;

pub mod replay;
#[cfg(test)]
mod storage_failure_tests {
    use super::*;
    #[test]
    fn failed_append_does_not_change_memory_and_poison_rejects_later_work() {
        let path = std::env::temp_dir().join(format!(
            "course-failure-{}-{}.jsonl",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut store = Store::open(&path).unwrap();
        let before = store.engine.snapshot();
        // Inject an OS write failure by replacing the owned writer with a read-only handle.
        store.file = File::open(&path).unwrap();
        assert!(store.commit(Event::Watchlist(vec!["TEST".into()])).is_err());
        assert_eq!(store.engine.snapshot(), before);
        assert!(store.poisoned);
        store.file = OpenOptions::new().append(true).open(&path).unwrap();
        assert!(store.commit(Event::Watchlist(vec!["TEST".into()])).is_err());
        drop(store);
        assert_eq!(Store::open(&path).unwrap().engine.snapshot(), before);
        std::fs::remove_file(path).unwrap();
    }
}

#[cfg(test)]
mod review_regressions {
    use super::*;
    #[test]
    fn recent_orders_follow_acceptance_order_not_identifier_sort() {
        let mut e = Engine::default();
        e.apply(Event::Tick(Tick {
            event_id: "t".into(),
            symbol: "RUST".into(),
            price_cents: 1,
            sequence: 1,
        }))
        .unwrap();
        for id in [
            "z-last-alphabetically",
            "a-first-alphabetically",
            "m-actually-latest",
        ] {
            e.apply(Event::Order(Order {
                order_id: id.into(),
                symbol: "RUST".into(),
                side: Side::Buy,
                quantity: 1,
            }))
            .unwrap();
        }
        assert_eq!(
            e.snapshot()
                .orders
                .iter()
                .map(|f| f.order_id.as_str())
                .collect::<Vec<_>>(),
            vec![
                "m-actually-latest",
                "a-first-alphabetically",
                "z-last-alphabetically"
            ]
        );
        for n in 0..101 {
            e.apply(Event::Order(Order {
                order_id: format!("later-{n}"),
                symbol: "RUST".into(),
                side: Side::Buy,
                quantity: 1,
            }))
            .unwrap();
        }
        assert_eq!(e.snapshot().orders.len(), 100);
        assert_eq!(e.snapshot().orders[0].order_id, "later-100");
        assert!(e.fill("z-last-alphabetically").is_some());
    }
    #[test]
    fn duplicate_durable_record_is_rejected_not_silently_counted() {
        let path = std::env::temp_dir().join(format!(
            "course-duplicate-log-{}-{}.jsonl",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let first = include_str!("../fixtures/legacy-v1.jsonl")
            .lines()
            .next()
            .unwrap();
        std::fs::write(&path, format!("{first}\n{first}\n")).unwrap();
        assert!(matches!(Store::open(&path),Err(error)if error.kind()==io::ErrorKind::InvalidData));
        std::fs::remove_file(path).unwrap();
    }
}
