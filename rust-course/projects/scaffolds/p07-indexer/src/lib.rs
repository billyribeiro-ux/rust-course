use std::io::{self, Read};
#[derive(Debug, PartialEq, Eq)] pub struct Counts { pub bytes: u64, pub lines: u64, pub words: u64 }
/// Stream with a fixed-size buffer. ASCII whitespace defines word boundaries.
/// An unterminated nonempty final line counts as one line. Exceeding max_bytes is InvalidData.
pub fn count(_reader: impl Read, _max_bytes: u64) -> io::Result<Counts> {
    todo!("Learner: preserve in-word and last-byte state across read boundaries")
}
#[cfg(test)] mod acceptance {
    use super::*;
    struct Tiny<'a>(&'a [u8]); impl Read for Tiny<'_> { fn read(&mut self, out: &mut [u8]) -> io::Result<usize> { if self.0.is_empty() || out.is_empty() { return Ok(0); } out[0] = self.0[0]; self.0 = &self.0[1..]; Ok(1) } }
    #[test] fn boundaries_are_not_words() { assert_eq!(count(Tiny(b"hello world\nlast"), 100).unwrap(), Counts { bytes: 16, lines: 2, words: 3 }); }
    #[test] fn empty_and_trailing_newline() { assert_eq!(count(&b""[..], 0).unwrap().lines, 0); assert_eq!(count(&b"a\n"[..], 2).unwrap().lines, 1); }
    #[test] fn budget_is_enforced_even_without_metadata() { assert_eq!(count(&b"12345"[..], 4).unwrap_err().kind(), io::ErrorKind::InvalidData); }
    #[test] fn io_failure_is_not_zero_bytes() { struct Broken; impl Read for Broken { fn read(&mut self, _: &mut [u8]) -> io::Result<usize> { Err(io::Error::other("injected")) } } assert!(count(Broken, 10).is_err()); }
}
