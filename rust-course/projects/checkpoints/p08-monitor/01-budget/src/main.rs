use std::time::Duration;
fn main() {
    let ms = Duration::from_millis;
    match checkpoint_p08_budget::retry_budget(ms(30), ms(100), ms(10), ms(80), 1, 3) {
        Some(budget) => println!("next_attempt_budget_ms={}", budget.as_millis()),
        None => println!("no retry fits"),
    }
}
