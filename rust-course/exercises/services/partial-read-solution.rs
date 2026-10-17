//! Worked solution: require the complete bounded payload before exposing text.
use std::io::{self, Read};
struct Fragmented {
    bytes: Vec<u8>,
    offset: usize,
}
impl Read for Fragmented {
    fn read(&mut self, target: &mut [u8]) -> io::Result<usize> {
        if self.offset == self.bytes.len() || target.is_empty() {
            return Ok(0);
        }
        target[0] = self.bytes[self.offset];
        self.offset += 1;
        Ok(1)
    }
}
fn decode(reader: &mut impl Read, len: usize) -> io::Result<String> {
    if !(1..=1024).contains(&len) {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "length budget"));
    }
    let mut bytes = vec![0; len];
    reader.read_exact(&mut bytes)?;
    String::from_utf8(bytes).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}
#[test]
fn a_frame_is_not_one_read() {
    let mut input = Fragmented {
        bytes: b"hello".to_vec(),
        offset: 0,
    };
    assert_eq!(decode(&mut input, 5).unwrap(), "hello");
}
#[test]
fn incomplete_payload_fails() {
    assert_eq!(
        decode(&mut &b"hi"[..], 3).unwrap_err().kind(),
        io::ErrorKind::UnexpectedEof
    );
}
