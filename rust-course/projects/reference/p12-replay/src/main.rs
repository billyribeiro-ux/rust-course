use std::{
    io::{self, Write},
    process::ExitCode,
};

fn count() -> io::Result<usize> {
    let mut args = std::env::args_os().skip(1);
    let count = args.next();
    if args.next().is_some() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "usage: p12-replay [EVENT_COUNT]",
        ));
    }
    let Some(count) = count else {
        return Ok(100_003);
    };
    let count = count.to_str().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "event count must be valid UTF-8",
        )
    })?;
    count.parse().map_err(|error| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("invalid event count: {error}"),
        )
    })
}

fn run() -> io::Result<()> {
    let replay = p12_replay::run(count()?).map_err(io::Error::other)?;
    let mut output = io::stdout().lock();
    writeln!(
        output,
        "events={} active={} submitted_notional_ticks={} digest={:016x}",
        replay.accepted(),
        replay.orders().len(),
        replay.submitted_notional_ticks(),
        replay.digest()
    )?;
    for (id, order) in replay.orders() {
        writeln!(
            output,
            "order={} {:?} price={} quantity={}",
            id.0, order.side, order.price, order.quantity
        )?;
    }
    output.flush()?;
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            let _ = writeln!(io::stderr().lock(), "replay: {error}");
            if error.kind() == io::ErrorKind::InvalidInput {
                ExitCode::from(2)
            } else {
                ExitCode::FAILURE
            }
        }
    }
}
