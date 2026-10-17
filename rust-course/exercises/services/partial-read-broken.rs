//! INTENTIONAL RUNTIME TEST FAILURE. Compiles, then a protocol assertion fails.
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
    let mut bytes = vec![0; len];
    // BUG: a single read is allowed to return fewer bytes than the full frame.
    let count = reader.read(&mut bytes)?;
    String::from_utf8(bytes[..count].to_vec())
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}
#[test]
fn a_frame_is_not_one_read() {
    let mut input = Fragmented {
        bytes: b"hello".to_vec(),
        offset: 0,
    };
    assert_eq!(decode(&mut input, 5).unwrap(), "hello");
}
