use p16_workbench::{Engine, Event, Order, Side, Tick};
fn main() {
    let mut engine = Engine::default();
    engine
        .apply(Event::Tick(Tick {
            event_id: "t1".into(),
            symbol: "RUST".into(),
            price_cents: 125,
            sequence: 1,
        }))
        .unwrap();
    let order = Event::Order(Order {
        order_id: "o1".into(),
        symbol: "RUST".into(),
        side: Side::Buy,
        quantity: 3,
    });
    println!("first applied: {}", engine.apply(order.clone()).unwrap());
    println!("retry applied: {}", engine.apply(order).unwrap());
    println!("cash cents: {}", engine.snapshot().cash_cents);
}
