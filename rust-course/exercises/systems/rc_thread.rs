use std::rc::Rc;
fn main() {
    let shared = Rc::new(String::from("index metadata"));
    std::thread::spawn(move || println!("{shared}"));
}
