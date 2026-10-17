use std::{
    io::{self, Write},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

fn write_progress(output: &mut impl Write, progress: p07_indexer::Progress) -> io::Result<()> {
    let phase = if progress.finished {
        "finished"
    } else if progress.received == 0 {
        "started"
    } else {
        "running"
    };
    writeln!(
        output,
        "progress {phase} received={}/{} indexed={} too_large={} failed={} cancelled={} cancellation_requested={}",
        progress.received,
        progress.total,
        progress.indexed,
        progress.too_large,
        progress.failed,
        progress.cancelled,
        progress.cancellation_requested
    )?;
    if let Some(submitted) = progress.submitted {
        writeln!(
            output,
            "progress submitted={submitted} without_result={} unsubmitted={}",
            submitted - progress.received,
            progress.total - submitted
        )?;
    }
    output.flush()
}

fn main() -> std::io::Result<()> {
    let paths = std::env::args_os()
        .skip(1)
        .map(Into::into)
        .collect::<Vec<_>>();
    if paths.is_empty() {
        let _ = writeln!(io::stderr().lock(), "usage: p07-indexer FILE ...");
        std::process::exit(2);
    }
    let cancel = Arc::new(AtomicBool::new(false));
    let mut progress_error = None;
    let mut progress_output = io::stderr().lock();
    let report = p07_indexer::index_with_progress(
        paths,
        p07_indexer::Config::default(),
        Arc::clone(&cancel),
        |progress| {
            if progress_error.is_none()
                && let Err(error) = write_progress(&mut progress_output, progress)
            {
                // Stop further work but still let index_with_progress drain and join.
                progress_error = Some(error);
                cancel.store(true, Ordering::Relaxed);
            }
        },
    )?;
    if let Some(error) = progress_error {
        return Err(error);
    }
    let mut output = io::stdout().lock();
    for e in report.entries {
        writeln!(
            output,
            "{} bytes={} lines={} words={} {:016x} {:?}",
            e.path.display(),
            e.bytes,
            e.lines,
            e.words,
            e.fingerprint,
            e.status
        )?;
    }
    writeln!(
        output,
        "submitted={} peak_workers={} cancelled={}",
        report.submitted, report.peak_workers, report.cancelled
    )?;
    output.flush()
}
