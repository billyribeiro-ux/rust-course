//! Bounded batch file indexer. Files are streamed; results and path metadata are capped.
use std::{
    fs::File,
    io::{self, Read},
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc,
    },
    thread,
};

#[derive(Clone, Copy, Debug)]
pub struct Config {
    pub workers: usize,
    pub queue: usize,
    pub max_files: usize,
    pub max_bytes: u64,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            workers: 4,
            queue: 8,
            max_files: 4096,
            max_bytes: 16 * 1024 * 1024,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    Indexed,
    TooLarge,
    Cancelled,
    Failed(io::ErrorKind),
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub path: PathBuf,
    pub input_index: usize,
    pub bytes: u64,
    pub lines: u64,
    pub words: u64,
    pub fingerprint: u64,
    pub status: Status,
}
#[derive(Debug)]
pub struct Report {
    pub entries: Vec<Entry>,
    pub submitted: usize,
    pub cancelled: bool,
    pub peak_workers: usize,
}

/// A coordinator-owned observation, not an estimate of bytes or time remaining.
/// `received` includes failed/partial results; only `indexed` counts whole files.
/// Counts are monotonic within a run. Completion order is not deterministic.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Progress {
    pub total: usize,
    pub received: usize,
    pub indexed: usize,
    pub too_large: usize,
    pub failed: usize,
    pub cancelled: usize,
    /// Known only after the producer has joined; queued work may have no result.
    pub submitted: Option<usize>,
    pub cancellation_requested: bool,
    /// All scoped work has joined and results have been sorted.
    pub finished: bool,
}
impl Progress {
    fn observe(&mut self, status: &Status) {
        // Input is capped at 4096 and each input has at most one returned entry.
        self.received += 1;
        match status {
            Status::Indexed => self.indexed += 1,
            Status::TooLarge => self.too_large += 1,
            Status::Failed(_) => self.failed += 1,
            Status::Cancelled => self.cancelled += 1,
        }
    }
}

fn count(path: PathBuf, input_index: usize, max_bytes: u64, cancel: &AtomicBool) -> Entry {
    let mut result = Entry {
        path,
        input_index,
        bytes: 0,
        lines: 0,
        words: 0,
        fingerprint: 0xcbf2_9ce4_8422_2325,
        status: Status::Indexed,
    };
    let run = |result: &mut Entry| -> io::Result<()> {
        let mut file = File::open(&result.path)?;
        let mut buf = [0u8; 8192];
        let mut in_word = false;
        let mut last = None;
        loop {
            if cancel.load(Ordering::Relaxed) {
                result.status = Status::Cancelled;
                return Ok(());
            }
            // Read at most the remaining budget plus one byte to detect an oversize file.
            let allowance = (max_bytes - result.bytes + 1).min(buf.len() as u64) as usize;
            let n = match file.read(&mut buf[..allowance]) {
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                other => other?,
            };
            if n == 0 {
                break;
            }
            if result.bytes + n as u64 > max_bytes {
                result.status = Status::TooLarge;
                return Ok(());
            }
            result.bytes += n as u64;
            for &byte in &buf[..n] {
                result.lines += u64::from(byte == b'\n');
                let word = !byte.is_ascii_whitespace();
                result.words += u64::from(word && !in_word);
                in_word = word;
                result.fingerprint =
                    (result.fingerprint ^ u64::from(byte)).wrapping_mul(0x100_0000_01b3);
                last = Some(byte);
            }
        }
        result.lines += u64::from(last.is_some_and(|byte| byte != b'\n'));
        Ok(())
    };
    if let Err(error) = run(&mut result) {
        result.status = Status::Failed(error.kind());
    }
    result
}

pub fn index(paths: Vec<PathBuf>, config: Config, cancel: Arc<AtomicBool>) -> io::Result<Report> {
    index_with_progress(paths, config, cancel, |_| {})
}

/// Index with an initial snapshot, one snapshot per received entry, and a final snapshot.
/// Invalid configuration returns before callbacks or threads start. The callback runs
/// synchronously on the calling coordinator, with no lock held and no extra queue.
/// A slow callback applies backpressure through the bounded result queue; it should
/// not wait for this same run to finish. Panics are programmer failures, not IO errors.
/// A caller can request cancellation through its retained `Arc<AtomicBool>`; pending
/// regular-file reads remain cooperative. On final progress, `submitted - received`
/// counts accepted jobs without a returned entry, and `total - submitted` counts jobs
/// never submitted. Neither category is silently counted as a successful file.
pub fn index_with_progress(
    paths: Vec<PathBuf>,
    config: Config,
    cancel: Arc<AtomicBool>,
    mut on_progress: impl FnMut(Progress),
) -> io::Result<Report> {
    if !(1..=32).contains(&config.workers)
        || !(1..=1024).contains(&config.queue)
        || config.max_files > 4096
        || paths.len() > config.max_files
        || config.max_bytes > 64 * 1024 * 1024
        || paths
            .iter()
            .any(|p| p.as_os_str().as_encoded_bytes().len() > 4096)
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "configuration or input exceeds documented bounds",
        ));
    }
    let mut progress = Progress {
        total: paths.len(),
        cancellation_requested: cancel.load(Ordering::Relaxed),
        ..Progress::default()
    };
    on_progress(progress);
    let (job_tx, job_rx) = mpsc::sync_channel::<(usize, PathBuf)>(config.queue);
    let (result_tx, result_rx) = mpsc::sync_channel(config.queue);
    let job_rx = Arc::new(Mutex::new(job_rx));
    let active = AtomicUsize::new(0);
    let peak = AtomicUsize::new(0);
    let (mut entries, submitted) = thread::scope(|scope| {
        let producer_cancel = cancel.clone();
        let producer = scope.spawn(move || {
            let mut submitted = 0;
            for (index, path) in paths.into_iter().enumerate() {
                if producer_cancel.load(Ordering::Relaxed) || job_tx.send((index, path)).is_err() {
                    break;
                }
                submitted += 1;
            }
            submitted
        });
        for _ in 0..config.workers {
            let jobs = job_rx.clone();
            let results = result_tx.clone();
            let cancel = cancel.clone();
            let active = &active;
            let peak = &peak;
            scope.spawn(move || {
                loop {
                    if cancel.load(Ordering::Relaxed) {
                        break;
                    }
                    // The receiver lock protects dequeue only; file I/O is outside the lock.
                    let next = jobs.lock().expect("receiver lock poisoned").recv();
                    let Ok((index, path)) = next else { break };
                    let now = active.fetch_add(1, Ordering::Relaxed) + 1;
                    peak.fetch_max(now, Ordering::Relaxed);
                    let entry = count(path, index, config.max_bytes, &cancel);
                    active.fetch_sub(1, Ordering::Relaxed);
                    if results.send(entry).is_err() {
                        break;
                    }
                }
            });
        }
        drop(job_rx);
        drop(result_tx);
        let mut entries = Vec::new();
        for entry in result_rx {
            progress.observe(&entry.status);
            progress.cancellation_requested = cancel.load(Ordering::Relaxed);
            entries.push(entry);
            on_progress(progress);
        }
        (entries, producer.join().expect("producer panicked"))
    });
    entries.sort_by(|a, b| a.path.cmp(&b.path).then(a.input_index.cmp(&b.input_index)));
    progress.submitted = Some(submitted);
    progress.cancellation_requested = cancel.load(Ordering::Relaxed);
    progress.finished = true;
    on_progress(progress);
    Ok(Report {
        entries,
        submitted,
        cancelled: progress.cancellation_requested,
        peak_workers: peak.load(Ordering::Relaxed),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    struct Scratch(PathBuf);
    impl Scratch {
        fn new() -> Self {
            let p = std::env::temp_dir().join(format!(
                "rust-course-index-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&p).unwrap();
            Self(p)
        }
    }
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn cancel() -> Arc<AtomicBool> {
        Arc::new(AtomicBool::new(false))
    }
    #[test]
    fn streaming_counts_across_buffer_boundaries_and_sorts() {
        let dir = Scratch::new();
        let a = dir.0.join("a");
        let z = dir.0.join("z");
        fs::write(&z, "x".repeat(9000) + " y\nlast").unwrap();
        fs::write(&a, b"").unwrap();
        let r = index(vec![z.clone(), a.clone()], Config::default(), cancel()).unwrap();
        assert_eq!(r.entries[0].path, a);
        assert_eq!(r.entries[0].lines, 0);
        assert_eq!(r.entries[1].path, z);
        assert_eq!((r.entries[1].lines, r.entries[1].words), (2, 3));
        assert!(r.peak_workers <= 4);
    }
    #[test]
    fn oversize_and_missing_files_are_per_file_results() {
        let dir = Scratch::new();
        let p = dir.0.join("big");
        fs::write(&p, b"12345").unwrap();
        let r = index(
            vec![p, dir.0.join("missing")],
            Config {
                max_bytes: 4,
                ..Config::default()
            },
            cancel(),
        )
        .unwrap();
        assert_eq!(r.entries[0].status, Status::TooLarge);
        assert_eq!(r.entries[1].status, Status::Failed(io::ErrorKind::NotFound));
    }
    #[test]
    fn cancellation_before_start_submits_nothing() {
        let r = index(
            vec!["unused".into()],
            Config::default(),
            Arc::new(AtomicBool::new(true)),
        )
        .unwrap();
        assert!(r.cancelled);
        assert!(r.entries.is_empty());
        assert_eq!(r.submitted, 0);
    }
    #[test]
    fn queue_one_does_not_deadlock_and_output_is_repeatable() {
        let dir = Scratch::new();
        let p = dir.0.join("file");
        fs::write(&p, b"a b\nc").unwrap();
        let c = Config {
            workers: 2,
            queue: 1,
            ..Config::default()
        };
        let one = index(vec![p.clone(); 40], c, cancel()).unwrap();
        let two = index(vec![p; 40], c, cancel()).unwrap();
        assert_eq!(one.entries, two.entries);
        assert_eq!(one.entries.len(), 40);
    }
    #[test]
    fn invalid_limits_are_rejected() {
        assert!(
            index(
                vec![],
                Config {
                    workers: 0,
                    ..Config::default()
                },
                cancel()
            )
            .is_err()
        );
    }

    #[test]
    fn progress_is_monotonic_and_reports_failures_without_calling_them_successes() {
        let dir = Scratch::new();
        let good = dir.0.join("good");
        let empty = dir.0.join("empty");
        let large = dir.0.join("large");
        fs::write(&good, b"a b").unwrap();
        fs::write(&empty, b"").unwrap();
        fs::write(&large, b"12345").unwrap();
        let mut observations = Vec::new();
        let coordinator = thread::current().id();
        let report = index_with_progress(
            vec![good, empty, large, dir.0.join("missing"), dir.0.clone()],
            Config {
                max_bytes: 4,
                queue: 1,
                ..Config::default()
            },
            cancel(),
            |progress| {
                assert_eq!(thread::current().id(), coordinator);
                observations.push(progress);
            },
        )
        .unwrap();
        assert_eq!(observations.len(), 7); // Initial, five results, final.
        assert_eq!(
            observations[0],
            Progress {
                total: 5,
                ..Progress::default()
            }
        );
        for pair in observations.windows(2) {
            assert!(pair[0].received <= pair[1].received);
            assert!(pair[0].indexed <= pair[1].indexed);
            assert!(pair[0].too_large <= pair[1].too_large);
            assert!(pair[0].failed <= pair[1].failed);
            assert_eq!(
                pair[1].received,
                pair[1].indexed + pair[1].too_large + pair[1].failed + pair[1].cancelled
            );
        }
        assert!(
            observations[..6]
                .iter()
                .all(|p| !p.finished && p.submitted.is_none())
        );
        let final_progress = observations[6];
        assert_eq!(
            (
                final_progress.received,
                final_progress.indexed,
                final_progress.too_large,
                final_progress.failed
            ),
            (5, 2, 1, 2)
        );
        assert!(final_progress.finished);
        assert_eq!(final_progress.submitted, Some(report.submitted));
        assert_eq!(report.entries.len(), final_progress.received);
    }

    #[test]
    fn initial_progress_can_cancel_before_any_file_is_opened() {
        let stopping = cancel();
        let mut observations = Vec::new();
        let report = index_with_progress(
            vec!["not-opened".into()],
            Config::default(),
            Arc::clone(&stopping),
            |progress| {
                observations.push(progress);
                stopping.store(true, Ordering::Relaxed);
            },
        )
        .unwrap();
        assert_eq!(observations.len(), 2);
        assert_eq!(report.submitted, 0);
        assert!(report.entries.is_empty());
        assert!(observations[1].finished && observations[1].cancellation_requested);
        assert_eq!(observations[1].submitted, Some(0));
        assert_eq!(observations[1].cancelled, 0); // Unsubmitted is not a cancelled result.
    }

    #[test]
    fn progress_cancellation_drains_results_and_accounts_for_unreturned_jobs() {
        let dir = Scratch::new();
        let file = dir.0.join("file");
        fs::write(&file, b"a b\n").unwrap();
        let stopping = cancel();
        let mut final_progress = None;
        let report = index_with_progress(
            vec![file; 32],
            Config {
                workers: 1,
                queue: 1,
                ..Config::default()
            },
            Arc::clone(&stopping),
            |progress| {
                if progress.received == 1 {
                    stopping.store(true, Ordering::Relaxed);
                }
                if progress.finished {
                    final_progress = Some(progress);
                }
            },
        )
        .unwrap();
        let p = final_progress.unwrap();
        assert!(p.cancellation_requested && report.cancelled);
        assert!(p.received >= 1 && p.received <= report.submitted);
        assert!(report.submitted < p.total);
        assert_eq!(
            p.indexed,
            report
                .entries
                .iter()
                .filter(|e| e.status == Status::Indexed)
                .count()
        );
        assert_eq!(
            p.cancelled,
            report
                .entries
                .iter()
                .filter(|e| e.status == Status::Cancelled)
                .count()
        );
        assert_eq!(p.received, p.indexed + p.cancelled);
        assert_eq!(
            p.received + (report.submitted - p.received) + (p.total - report.submitted),
            p.total
        );
        // Callback cancellation occurs while collection is in progress. Exact counts
        // depend on scheduling; the conservation equation does not.
    }

    #[test]
    fn empty_work_has_initial_and_final_progress() {
        let mut observations = Vec::new();
        let report = index_with_progress(vec![], Config::default(), cancel(), |p| {
            observations.push(p)
        })
        .unwrap();
        assert_eq!(observations.len(), 2);
        assert_eq!(observations[0], Progress::default());
        assert!(observations[1].finished);
        assert_eq!(observations[1].submitted, Some(0));
        assert!(report.entries.is_empty());
    }

    #[test]
    fn invalid_configuration_emits_no_misleading_started_event() {
        let mut calls = 0;
        let result = index_with_progress(
            vec![],
            Config {
                workers: 0,
                ..Config::default()
            },
            cancel(),
            |_| calls += 1,
        );
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::InvalidInput);
        assert_eq!(calls, 0);
    }
}
