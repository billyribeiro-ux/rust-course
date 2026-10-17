//! Bounded serial counting before any worker-pool coordination is introduced.
use std::io::{self, Read};
#[derive(Debug, PartialEq, Eq)]
pub struct Counts {
    pub bytes: u64,
    pub lines: u64,
    pub words: u64,
}
pub fn count(mut reader: impl Read, max_bytes: u64) -> io::Result<Counts> {
    if max_bytes > 64 * 1024 * 1024 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "budget exceeds 64 MiB",
        ));
    }
    let mut result = Counts {
        bytes: 0,
        lines: 0,
        words: 0,
    };
    let mut buffer = [0u8; 8192];
    let mut in_word = false;
    let mut last = None;
    loop {
        let allowance = (max_bytes - result.bytes + 1).min(buffer.len() as u64) as usize;
        let used = match reader.read(&mut buffer[..allowance]) {
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            other => other?,
        };
        if used == 0 {
            break;
        }
        if result.bytes + used as u64 > max_bytes {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "input exceeds byte budget",
            ));
        }
        result.bytes += used as u64;
        for &byte in &buffer[..used] {
            result.lines += u64::from(byte == b'\n');
            let word = !byte.is_ascii_whitespace();
            result.words += u64::from(word && !in_word);
            in_word = word;
            last = Some(byte);
        }
    }
    result.lines += u64::from(last.is_some_and(|byte| byte != b'\n'));
    Ok(result)
}
#[cfg(test)]
mod tests {
    use super::*;
    struct Tiny<'a>(&'a [u8]);
    impl Read for Tiny<'_> {
        fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
            if self.0.is_empty() || out.is_empty() {
                return Ok(0);
            }
            out[0] = self.0[0];
            self.0 = &self.0[1..];
            Ok(1)
        }
    }
    #[test]
    fn chunk_boundaries_preserve_words_and_lines() {
        assert_eq!(
            count(Tiny(b"hello world\nlast"), 16).unwrap(),
            Counts {
                bytes: 16,
                lines: 2,
                words: 3
            }
        );
        assert_eq!(count(&b"a\n"[..], 2).unwrap().lines, 1);
        assert_eq!(
            count(&b""[..], 0).unwrap(),
            Counts {
                bytes: 0,
                lines: 0,
                words: 0
            }
        );
    }
    #[test]
    fn detects_one_byte_over_budget() {
        assert_eq!(
            count(&b"12345"[..], 4).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
        assert_eq!(
            count(&b"a"[..], 0).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
        assert_eq!(
            count(&b""[..], u64::MAX).unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
    }
    #[test]
    fn interrupted_reads_retry_but_real_errors_propagate() {
        struct Interruption(bool);
        impl Read for Interruption {
            fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
                if !self.0 {
                    self.0 = true;
                    Err(io::ErrorKind::Interrupted.into())
                } else {
                    Err(io::ErrorKind::PermissionDenied.into())
                }
            }
        }
        assert_eq!(
            count(Interruption(false), 100).unwrap_err().kind(),
            io::ErrorKind::PermissionDenied
        );
    }
}
