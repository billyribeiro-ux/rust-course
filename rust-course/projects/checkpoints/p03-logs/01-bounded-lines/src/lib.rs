use std::io::{self, BufRead};
#[derive(Debug, PartialEq, Eq)]
pub enum Line {
    End,
    Bytes(Vec<u8>),
    Oversized,
}
pub fn read_line(reader: &mut impl BufRead, max: usize) -> io::Result<Line> {
    if !(1..=4096).contains(&max) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "line bound must be 1..=4096",
        ));
    }
    let mut bytes = Vec::new();
    let mut oversized = false;
    loop {
        let chunk = match reader.fill_buf() {
            Ok(chunk) => chunk,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        };
        if chunk.is_empty() {
            return Ok(if oversized {
                Line::Oversized
            } else if bytes.is_empty() {
                Line::End
            } else {
                Line::Bytes(bytes)
            });
        }
        let newline = chunk.iter().position(|&byte| byte == b'\n');
        let payload_len = newline.unwrap_or(chunk.len());
        if !oversized {
            let room = max - bytes.len();
            let retain = room.min(payload_len);
            bytes.extend_from_slice(&chunk[..retain]);
            oversized = payload_len > room;
        }
        let consumed = payload_len + usize::from(newline.is_some());
        reader.consume(consumed);
        if newline.is_some() {
            return Ok(if oversized {
                Line::Oversized
            } else {
                Line::Bytes(bytes)
            });
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufReader, Cursor};
    #[test]
    fn chunk_boundaries_do_not_change_lines() {
        for capacity in 1..=8 {
            let mut reader = BufReader::with_capacity(capacity, Cursor::new(b"abc\nlast"));
            assert_eq!(
                read_line(&mut reader, 4).unwrap(),
                Line::Bytes(b"abc".to_vec())
            );
            assert_eq!(
                read_line(&mut reader, 4).unwrap(),
                Line::Bytes(b"last".to_vec())
            );
            assert_eq!(read_line(&mut reader, 4).unwrap(), Line::End);
        }
    }
    #[test]
    fn oversized_line_is_drained() {
        let mut reader = &b"12345\nok\n"[..];
        assert_eq!(read_line(&mut reader, 4).unwrap(), Line::Oversized);
        assert_eq!(
            read_line(&mut reader, 4).unwrap(),
            Line::Bytes(b"ok".to_vec())
        );
    }
    #[test]
    fn empty_line_and_exact_bound_are_distinct_from_eof() {
        let mut reader = &b"\n1234\n"[..];
        assert_eq!(read_line(&mut reader, 4).unwrap(), Line::Bytes(vec![]));
        assert_eq!(
            read_line(&mut reader, 4).unwrap(),
            Line::Bytes(b"1234".to_vec())
        );
        assert_eq!(read_line(&mut reader, 4).unwrap(), Line::End);
    }
    #[test]
    fn io_failure_is_not_end() {
        struct Broken;
        impl std::io::Read for Broken {
            fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
                Err(io::Error::other("injected"))
            }
        }
        let mut reader = BufReader::new(Broken);
        assert!(read_line(&mut reader, 4).is_err());
    }
}
