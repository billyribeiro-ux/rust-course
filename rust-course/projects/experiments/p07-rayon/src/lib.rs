//! CPU-only fingerprints over an explicitly bounded, already-read batch.
//! This experiment does not perform file I/O or replace the streaming indexer.
use rayon::prelude::*;
use std::{fmt, io, thread};

pub const MAX_FILES: usize = 128;
pub const MAX_FILE_BYTES: usize = 64 * 1024;
pub const MAX_WORKERS: usize = 8;

#[derive(Debug)]
pub enum Error {
    InvalidBounds,
    ThreadSpawn(io::Error),
    ThreadPanicked,
    PoolBuild(rayon::ThreadPoolBuildError),
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidBounds => f.write_str(
                "batch exceeds 128 files or 64 KiB per file, or workers are outside 1..=8",
            ),
            Self::ThreadSpawn(error) => write!(f, "could not start scoped worker: {error}"),
            Self::ThreadPanicked => f.write_str("a scoped CPU worker panicked"),
            Self::PoolBuild(error) => write!(f, "could not construct Rayon pool: {error}"),
        }
    }
}
impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::ThreadSpawn(error) => Some(error),
            Self::PoolBuild(error) => Some(error),
            _ => None,
        }
    }
}

fn validate(inputs: &[Vec<u8>]) -> Result<(), Error> {
    if inputs.len() > MAX_FILES || inputs.iter().any(|bytes| bytes.len() > MAX_FILE_BYTES) {
        Err(Error::InvalidBounds)
    } else {
        Ok(())
    }
}
fn validate_workers(workers: usize) -> Result<(), Error> {
    if (1..=MAX_WORKERS).contains(&workers) {
        Ok(())
    } else {
        Err(Error::InvalidBounds)
    }
}

pub fn fixture(files: usize, bytes_per_file: usize) -> Result<Vec<Vec<u8>>, Error> {
    if files > MAX_FILES || bytes_per_file > MAX_FILE_BYTES {
        return Err(Error::InvalidBounds);
    }
    Ok((0..files)
        .map(|file| {
            (0..bytes_per_file)
                .map(|index| ((file * 17 + index * 31) % 251) as u8)
                .collect()
        })
        .collect())
}

/// Same noncryptographic FNV-1a fingerprint convention as the reference indexer.
/// A fingerprint can collide; it is not evidence that unknown files are identical.
pub fn fingerprint(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x100_0000_01b3)
    })
}

pub fn serial(inputs: &[Vec<u8>]) -> Result<Vec<u64>, Error> {
    validate(inputs)?;
    Ok(inputs.iter().map(|bytes| fingerprint(bytes)).collect())
}

pub fn scoped_threads(inputs: &[Vec<u8>], workers: usize) -> Result<Vec<u64>, Error> {
    validate(inputs)?;
    validate_workers(workers)?;
    if inputs.is_empty() {
        return Ok(Vec::new());
    }
    let chunk_size = inputs.len().div_ceil(workers);
    thread::scope(|scope| {
        let mut handles = Vec::with_capacity(workers);
        let mut error = None;
        for (index, chunk) in inputs.chunks(chunk_size).enumerate() {
            let task = thread::Builder::new()
                .name(format!("fingerprint-{index}"))
                .spawn_scoped(scope, move || {
                    chunk
                        .iter()
                        .map(|bytes| fingerprint(bytes))
                        .collect::<Vec<_>>()
                });
            match task {
                Ok(handle) => handles.push(handle),
                Err(cause) => {
                    error = Some(Error::ThreadSpawn(cause));
                    break;
                }
            }
        }
        let mut output = Vec::with_capacity(inputs.len());
        // Join every started worker, even if startup or another worker failed.
        // Joining in chunk order preserves the input order in the output.
        for handle in handles {
            match handle.join() {
                Ok(values) => output.extend(values),
                Err(_) => {
                    if error.is_none() {
                        error = Some(Error::ThreadPanicked);
                    }
                }
            }
        }
        match error {
            Some(error) => Err(error),
            None => Ok(output),
        }
    })
}

pub struct ParallelResult {
    pub fingerprints: Vec<u64>,
    pub pool_threads: usize,
}
pub fn rayon_pool(inputs: &[Vec<u8>], workers: usize) -> Result<ParallelResult, Error> {
    validate(inputs)?;
    validate_workers(workers)?;
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .map_err(Error::PoolBuild)?;
    let fingerprints = pool.install(|| inputs.par_iter().map(|bytes| fingerprint(bytes)).collect());
    Ok(ParallelResult {
        fingerprints,
        pool_threads: pool.current_num_threads(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_strategy_returns_the_same_ordered_output() {
        for (files, bytes) in [(0, 0), (1, 1), (3, 37), (17, 1025), (128, 3)] {
            let inputs = fixture(files, bytes).unwrap();
            let expected = serial(&inputs).unwrap();
            for workers in [1, 2, 4, 8] {
                assert_eq!(scoped_threads(&inputs, workers).unwrap(), expected);
                let actual = rayon_pool(&inputs, workers).unwrap();
                assert_eq!(actual.fingerprints, expected);
                assert_eq!(actual.pool_threads, workers);
            }
        }
    }
    #[test]
    fn bounds_are_enforced_before_scheduling() {
        for workers in [0, 9, usize::MAX] {
            assert!(matches!(
                scoped_threads(&[], workers),
                Err(Error::InvalidBounds)
            ));
            assert!(matches!(
                rayon_pool(&[], workers),
                Err(Error::InvalidBounds)
            ));
        }
        assert!(fixture(129, 0).is_err());
        assert!(fixture(0, MAX_FILE_BYTES + 1).is_err());
        assert!(serial(&vec![vec![]; 129]).is_err());
        assert!(serial(&[vec![0; MAX_FILE_BYTES + 1]]).is_err());
    }
    #[test]
    fn fingerprint_convention_has_fixed_oracles() {
        assert_eq!(fingerprint(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fingerprint(b"a"), 0xaf63_dc4c_8601_ec8c);
    }
}
