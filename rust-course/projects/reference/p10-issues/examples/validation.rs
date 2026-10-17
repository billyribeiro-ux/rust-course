fn main() {
    for title in ["  fix login  ", "  "] {
        match p10_issues::validate_title(title) {
            Ok(value) => println!("accepted: {value}"),
            Err(error) => println!("rejected: {}", error.1),
        }
    }
}
