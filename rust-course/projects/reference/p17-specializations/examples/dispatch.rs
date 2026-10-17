use rust_specializations::{Countdown, KindName, Packet, drain, with_first};
fn main() {
    println!(
        "{} {:?} {}",
        Packet::<u8, 4>::kind_name(),
        drain(&mut Countdown(3)),
        with_first("alpha beta", |s| s.split_whitespace().next().unwrap_or(""))
    );
}
