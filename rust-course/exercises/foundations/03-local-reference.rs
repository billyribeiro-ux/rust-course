fn title() -> &'static str { let text = String::from("temporary"); &text }
fn main() { println!("{}", title()); }
