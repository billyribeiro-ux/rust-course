// INTENTIONAL LOGIC BUG: atomic accesses prevent a data race, not over-admission.
// Expected output: accepted=2 remaining=0 even though capacity was only1.
use std::sync::{
    Barrier,
    atomic::{AtomicU32, Ordering},
};
fn main() {
    let remaining = AtomicU32::new(1);
    let both_loaded = Barrier::new(2);
    let accepted = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..2)
            .map(|_| {
                scope.spawn(|| {
                    let old = remaining.load(Ordering::SeqCst);
                    both_loaded.wait(); // force both workers to observe1 before either writes
                    if old > 0 {
                        remaining.store(old - 1, Ordering::SeqCst);
                        true
                    } else {
                        false
                    }
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|h| u32::from(h.join().expect("fixture worker cannot panic")))
            .sum::<u32>()
    });
    println!(
        "accepted={accepted} remaining={}",
        remaining.load(Ordering::SeqCst)
    );
}
