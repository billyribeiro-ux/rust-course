//! Bounded synchronous framing checkpoint, before Tokio or a chat service.
use std::io::{self, Read, Write};
pub const MAX_FRAME: usize = 1024;
fn invalid(reason: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, reason)
}
pub fn encode(mut output: impl Write, text: &str) -> io::Result<()> {
    if text.is_empty() || text.len() > MAX_FRAME || text.chars().any(char::is_control) {
        return Err(invalid("invalid frame text"));
    }
    output.write_all(&(text.len() as u32).to_be_bytes())?;
    output.write_all(text.as_bytes())
}
pub fn decode(mut input: impl Read) -> io::Result<Option<String>> {
    let mut header = [0; 4];
    match input.read_exact(&mut header[..1]) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(error) => return Err(error),
    }
    input.read_exact(&mut header[1..])?;
    let length = u32::from_be_bytes(header) as usize;
    if !(1..=MAX_FRAME).contains(&length) {
        return Err(invalid("frame length outside budget"));
    }
    let mut payload = vec![0; length];
    input.read_exact(&mut payload)?;
    let text = String::from_utf8(payload).map_err(|_| invalid("invalid UTF-8"))?;
    if text.chars().any(char::is_control) {
        return Err(invalid("control characters"));
    }
    Ok(Some(text))
}
#[cfg(test)]
mod tests {
    use super::*;
    struct Fragmented<'a>(&'a [u8]);
    impl Read for Fragmented<'_> {
        fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
            let take = output.len().min(1);
            self.0.read(&mut output[..take])
        }
    }
    #[test]
    fn fragmented_utf8_roundtrip() {
        let mut bytes = Vec::new();
        encode(&mut bytes, "café").unwrap();
        assert_eq!(&bytes[..4], &[0, 0, 0, 5]);
        assert_eq!(decode(Fragmented(&bytes)).unwrap(), Some("café".into()));
    }
    #[test]
    fn clean_eof_differs_from_truncation() {
        assert_eq!(decode(&[][..]).unwrap(), None);
        assert_eq!(
            decode(&[0, 0][..]).unwrap_err().kind(),
            io::ErrorKind::UnexpectedEof
        );
        assert_eq!(
            decode(&[0, 0, 0, 2, b'x'][..]).unwrap_err().kind(),
            io::ErrorKind::UnexpectedEof
        );
    }
    #[test]
    fn reject_before_unbounded_allocation() {
        assert!(decode(&[255, 255, 255, 255][..]).is_err());
        assert!(decode(&[0, 0, 0, 1, 255][..]).is_err());
        assert!(decode(&[0, 0, 0, 1, b'\n'][..]).is_err());
        assert!(encode(Vec::new(), "hello\n").is_err());
    }
}
