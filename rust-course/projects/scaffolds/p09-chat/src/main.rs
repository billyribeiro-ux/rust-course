// Learner checkpoint 0: encode one frame; add a bounded decoder next.
fn encode(text: &str) -> Result<Vec<u8>, &'static str> {
    if text.is_empty() || text.len() > 1024 {
        return Err("length outside 1..=1024");
    }
    if text.chars().any(char::is_control) {
        return Err("control characters are not allowed");
    }
    let mut bytes = (text.len() as u32).to_be_bytes().to_vec();
    bytes.extend_from_slice(text.as_bytes());
    Ok(bytes)
}
fn main() {
    println!("{:?}", encode("hello").expect("valid fixture"));
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn control_characters_are_rejected() {
        assert!(encode("hello\n").is_err());
        assert!(encode("hello\u{007f}").is_err());
    }
    #[test]
    fn length_is_bytes() {
        assert_eq!(&encode("é").unwrap()[..4], &[0, 0, 0, 2]);
    }
}
