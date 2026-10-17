enum Status { Open, Done }
fn label(s: Status) -> &'static str { match s { Status::Open => "open" } }
fn main() { println!("{}", label(Status::Done)); }
