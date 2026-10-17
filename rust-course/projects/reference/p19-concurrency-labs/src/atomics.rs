use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
};

/// A single-counter budget. No other memory is published by taking a unit.
pub struct Budget(AtomicU32);
impl Budget {
    pub fn new(units: u32) -> Self {
        Self(AtomicU32::new(units))
    }
    pub fn remaining(&self) -> u32 {
        self.0.load(Ordering::Relaxed)
    }
    pub fn try_take(&self, amount: u32) -> bool {
        let mut observed = self.0.load(Ordering::Relaxed);
        loop {
            let Some(next) = observed.checked_sub(amount) else {
                return false;
            };
            match self
                .0
                .compare_exchange_weak(observed, next, Ordering::Relaxed, Ordering::Relaxed)
            {
                Ok(_) => return true,
                Err(actual) => observed = actual,
            }
        }
    }
}

struct Shared {
    files: AtomicU64,
    bytes: AtomicU64,
    ready: AtomicBool,
}
/// Exactly one publication: this owner is intentionally not Clone.
pub struct Publisher(Arc<Shared>);
pub struct Reader(Arc<Shared>);
pub fn publication() -> (Publisher, Reader) {
    let shared = Arc::new(Shared {
        files: AtomicU64::new(0),
        bytes: AtomicU64::new(0),
        ready: AtomicBool::new(false),
    });
    (Publisher(Arc::clone(&shared)), Reader(shared))
}
impl Publisher {
    /// Consuming self prevents a second write after ready becomes true.
    pub fn publish(self, files: u64, bytes: u64) {
        self.0.files.store(files, Ordering::Relaxed);
        self.0.bytes.store(bytes, Ordering::Relaxed);
        self.0.ready.store(true, Ordering::Release);
    }
}
impl Reader {
    /// Nonblocking. None means no published summary has been observed.
    pub fn snapshot(&self) -> Option<(u64, u64)> {
        if !self.0.ready.load(Ordering::Acquire) {
            return None;
        }
        Some((
            self.0.files.load(Ordering::Relaxed),
            self.0.bytes.load(Ordering::Relaxed),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn budget_never_admits_more_than_available() {
        let budget = Budget::new(100);
        let accepted: u32 = std::thread::scope(|scope| {
            let handles: Vec<_> = (0..16)
                .map(|_| scope.spawn(|| (0..50).filter(|_| budget.try_take(1)).count() as u32))
                .collect();
            handles
                .into_iter()
                .map(|h| h.join().expect("bounded fixture worker does not panic"))
                .sum()
        });
        assert_eq!(accepted, 100);
        assert_eq!(budget.remaining(), 0);
        assert!(!budget.try_take(1));
        assert!(budget.try_take(0));
    }
    #[test]
    fn oversized_reservation_leaves_the_budget_unchanged() {
        let budget = Budget::new(3);
        assert!(!budget.try_take(u32::MAX));
        assert_eq!(budget.remaining(), 3);
        assert!(budget.try_take(3));
    }
    #[test]
    fn no_partial_summary_is_exposed_by_the_api() {
        let (publisher, reader) = publication();
        assert_eq!(reader.snapshot(), None);
        publisher.publish(3, 120);
        assert_eq!(reader.snapshot(), Some((3, 120)));
        assert_eq!(reader.snapshot(), Some((3, 120)));
    }
    #[test]
    fn reader_survives_publisher_drop_without_fabricating_a_summary() {
        let (publisher, reader) = publication();
        drop(publisher);
        assert_eq!(reader.snapshot(), None);
    }
}
