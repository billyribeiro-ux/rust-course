//! Start with a pure replay boundary; integrate it into the reference storage shell later.
#[derive(Debug, PartialEq, Eq)]
pub enum ReplayError { Corrupt }
/// Input is length:u8, payload:length bytes, checksum:u8 (wrapping byte sum).
/// Return complete payloads and the byte offset after the last valid record.
/// Incomplete final record is ignored; complete checksum mismatch is an error.
pub fn replay(_input: &[u8]) -> Result<(Vec<Vec<u8>>, usize), ReplayError> {
    todo!("Learner: check bounds before slices, validate complete records, preserve prefix boundary")
}
#[cfg(test)] mod acceptance {
    use super::*;
    #[test] fn complete_empty_and_torn() { assert_eq!(replay(&[]), Ok((vec![], 0))); assert_eq!(replay(&[2, 4, 5, 9, 3, 8]), Ok((vec![vec![4, 5]], 4))); }
    #[test] fn full_corruption_is_an_error() { assert_eq!(replay(&[2, 4, 5, 8]), Err(ReplayError::Corrupt)); }
    #[test] fn zero_length_and_wrapping_checksum_are_valid() { assert_eq!(replay(&[0, 0, 2, 255, 2, 1]), Ok((vec![vec![], vec![255, 2]], 6))); }
    #[test] fn every_cut_preserves_complete_prefix() { for cut in 1..4 { let bytes = [1, 7, 7, 2, 4, 5, 9]; assert_eq!(replay(&bytes[..3 + cut]), Ok((vec![vec![7]], 3))); } }
}
