//! Replay an explicitly bounded synthetic tick fixture; no live-market integration.
use p12_replay::ticks::{self, Limits};
use std::{
    fs::File,
    io::{self, BufReader, Write},
    path::PathBuf,
    process::ExitCode,
};

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let [path, rest @ ..] = args.as_slice() else {
        return Err("usage: tick_replay PATH [BATCH_SIZE 1..=1024]".into());
    };
    let batch = match rest {
        [] => Limits::default().batch,
        [batch] => batch
            .to_str()
            .ok_or("batch size must be UTF-8 integer text")?
            .parse::<usize>()?,
        _ => return Err("usage: tick_replay PATH [BATCH_SIZE 1..=1024]".into()),
    };
    if !(1..=ticks::MAX_BATCH).contains(&batch) {
        return Err("batch size must be 1..=1024".into());
    }
    let file = File::open(PathBuf::from(path))?;
    let summary = ticks::replay(
        BufReader::with_capacity(4096, file),
        Limits {
            batch,
            ..Limits::default()
        },
    )?;
    let mut output = io::stdout().lock();
    writeln!(
        output,
        "SIMULATED TICKS: events={} first_timestamp_ms={:?} last_timestamp_ms={:?}",
        summary.events(),
        summary.first_timestamp_ms(),
        summary.last_timestamp_ms()
    )?;
    writeln!(
        output,
        "symbol,count,volume,notional_ticks,first,last,low,high"
    )?;
    for (symbol, a) in summary.symbols() {
        writeln!(
            output,
            "{symbol},{},{},{},{},{},{},{}",
            a.count, a.volume, a.notional_ticks, a.first, a.last, a.low, a.high
        )?;
    }
    output.flush()?;
    Ok(())
}
fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            // Errors are visible; a failed replay emits no successful partial report.
            let _ = writeln!(io::stderr().lock(), "tick replay: {error}");
            ExitCode::FAILURE
        }
    }
}
