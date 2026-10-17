use std::fmt;
#[derive(Debug, PartialEq, Eq)]
pub struct Replay {
    pub payloads: Vec<Vec<u8>>,
    pub valid_bytes: usize,
    pub torn_tail: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    InputLimit,
    Checksum { offset: usize },
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InputLimit => f.write_str("input exceeds 65536 bytes"),
            Self::Checksum { offset } => write!(f, "complete checksum mismatch at byte {offset}"),
        }
    }
}
impl std::error::Error for Error {}
pub fn replay(input: &[u8]) -> Result<Replay, Error> {
    if input.len() > 65536 {
        return Err(Error::InputLimit);
    }
    let mut result = Replay {
        payloads: Vec::new(),
        valid_bytes: 0,
        torn_tail: false,
    };
    while result.valid_bytes < input.len() {
        let start = result.valid_bytes;
        let length = usize::from(input[start]);
        let record_length = length + 2;
        if input.len() - start < record_length {
            result.torn_tail = true;
            return Ok(result);
        }
        let payload = &input[start + 1..start + 1 + length];
        let checksum = payload
            .iter()
            .fold(0u8, |sum, &byte| sum.wrapping_add(byte));
        if input[start + 1 + length] != checksum {
            return Err(Error::Checksum { offset: start });
        }
        result.payloads.push(payload.to_vec());
        result.valid_bytes += record_length;
    }
    Ok(result)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn complete_empty_and_zero_length() {
        assert_eq!(
            replay(&[]).unwrap(),
            Replay {
                payloads: vec![],
                valid_bytes: 0,
                torn_tail: false
            }
        );
        assert_eq!(replay(&[0, 0]).unwrap().payloads, vec![Vec::<u8>::new()]);
    }
    #[test]
    fn every_incomplete_cut_preserves_valid_prefix() {
        let bytes = [1, 7, 7, 2, 4, 5, 9];
        for end in 4..7 {
            let state = replay(&bytes[..end]).unwrap();
            assert_eq!(state.payloads, vec![vec![7]]);
            assert_eq!(state.valid_bytes, 3);
            assert!(state.torn_tail);
        }
        assert!(!replay(&bytes).unwrap().torn_tail);
    }
    #[test]
    fn complete_damage_is_not_a_tail() {
        assert_eq!(replay(&[1, 7, 8]), Err(Error::Checksum { offset: 0 }));
        assert_eq!(
            replay(&[2, 255, 2, 1]).unwrap().payloads,
            vec![vec![255, 2]]
        );
    }
    #[test]
    fn input_budget_is_checked() {
        assert_eq!(replay(&vec![0; 65537]), Err(Error::InputLimit));
        assert_eq!(replay(&vec![0; 65536]).unwrap().valid_bytes, 65536);
    }
}
