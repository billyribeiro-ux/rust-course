use concurrency_labs::atomics::{Budget, publication};
fn main() {
    let budget = Budget::new(2);
    println!(
        "take2={} take1={} remaining={}",
        budget.try_take(2),
        budget.try_take(1),
        budget.remaining()
    );
    let (publisher, reader) = publication();
    println!("before={:?}", reader.snapshot());
    publisher.publish(3, 120);
    println!("after={:?}", reader.snapshot());
}
