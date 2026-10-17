//! Exact journal admission boundaries, without sleeps, network access or fake durability claims.
use p16_workbench::{CommitError, Event, Store};
use std::{fs, path::PathBuf};
struct Scratch(PathBuf);
impl Scratch {
    fn new(label: &str) -> Self {
        // Wall-clock timestamps can repeat across concurrent test threads.
        // Atomic allocation plus create_dir's no-overwrite check establishes ownership.
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        loop {
            let sequence = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "course-workbench-persistence-{label}-{}-{sequence}",
                std::process::id()
            ));
            match fs::create_dir(&path) {
                Ok(()) => return Self(path),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => panic!("cannot create test directory: {error}"),
            }
        }
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn record(position: u64) -> String {
    format!(
        "{{\"version\":2,\"position\":{position},\"event\":{{\"kind\":\"watchlist\",\"data\":[]}}}}\n"
    )
}
#[test]
fn exact_record_size_is_accepted_but_one_extra_byte_fails_without_repair() {
    let directory = Scratch::new("bytes");
    let path = directory.0.join("events.jsonl");
    let mut exact = record(1).into_bytes();
    exact.pop();
    exact.resize(16383, b' ');
    exact.push(b'\n');
    assert_eq!(exact.len(), 16384);
    fs::write(&path, &exact).unwrap();
    assert_eq!(Store::open(&path).unwrap().snapshot().events, 1);
    let mut oversized = exact.clone();
    oversized.insert(oversized.len() - 1, b' ');
    fs::write(&path, &oversized).unwrap();
    assert!(Store::open(&path).is_err());
    assert_eq!(fs::read(&path).unwrap(), oversized);
    exact.pop();
    fs::write(&path, &exact).unwrap();
    assert!(Store::open(&path).is_err());
    assert_eq!(fs::read(&path).unwrap(), exact);
}
#[test]
fn physical_record_limit_survives_restart_and_rejects_before_append() {
    let directory = Scratch::new("records");
    let path = directory.0.join("events.jsonl");
    // Generate a complete legal prefix in one write; this checks replay/admission,
    // not ten thousand acknowledged fsync operations or simulated power failure.
    let prefix = (1..=10000).map(record).collect::<String>();
    fs::write(&path, &prefix).unwrap();
    let mut store = Store::open(&path).unwrap();
    assert_eq!(store.snapshot().events, 10000);
    assert!(matches!(
        store.commit(Event::Watchlist(vec!["RUST".into()])),
        Err(CommitError::Rejected(_))
    ));
    assert_eq!(fs::read_to_string(&path).unwrap(), prefix);
    drop(store);
    let too_many = prefix + &record(10001);
    fs::write(&path, &too_many).unwrap();
    assert!(Store::open(&path).is_err());
    assert_eq!(fs::read_to_string(&path).unwrap(), too_many);
}

#[test]
fn valid_looking_price_damage_is_outside_the_integrity_contract() {
    use p16_workbench::{Order, Side, Tick};
    let directory = Scratch::new("valid-damage");
    let path = directory.0.join("events.jsonl");
    let original_cash;
    {
        let mut store = Store::open(&path).unwrap();
        store
            .commit(Event::Tick(Tick {
                event_id: "tick-1".into(),
                symbol: "RUST".into(),
                price_cents: 125,
                sequence: 1,
            }))
            .unwrap();
        store
            .commit(Event::Order(Order {
                order_id: "order-1".into(),
                symbol: "RUST".into(),
                side: Side::Buy,
                quantity: 3,
            }))
            .unwrap();
        original_cash = store.snapshot().cash_cents;
    }
    let original = fs::read_to_string(&path).unwrap();
    assert_eq!(original.matches("\"price_cents\":125").count(), 1);
    let damaged = original.replace("\"price_cents\":125", "\"price_cents\":126");
    fs::write(&path, &damaged).unwrap();
    let recovered = Store::open(&path).unwrap();
    assert_eq!(recovered.snapshot().cash_cents, original_cash - 3);
    assert_eq!(fs::read_to_string(&path).unwrap(), damaged);
}
