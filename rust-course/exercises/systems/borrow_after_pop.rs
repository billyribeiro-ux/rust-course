use std::collections::VecDeque;
fn main() {
    let mut queue = VecDeque::from([String::from("one")]);
    let borrowed = queue.front().unwrap();
    queue.pop_front();
    println!("{borrowed}");
}
