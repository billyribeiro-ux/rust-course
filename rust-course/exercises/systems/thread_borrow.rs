fn main() {
    let title = String::from("local owner");
    std::thread::spawn(|| println!("{title}"));
}
