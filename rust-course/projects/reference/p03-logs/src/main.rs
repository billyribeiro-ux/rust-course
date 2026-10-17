use log_detective::{Limits, Report, analyze, search};
use std::{
    ffi::OsString,
    fs::File,
    io::{self, BufRead, BufReader, Write},
    process::ExitCode,
};

const USAGE: &str = "usage: log-detective [--contains LITERAL] [--max-bytes N] [--] <file|->\n\
    Search is case-sensitive and emits valid matching records to stdout.\n\
    Search summary goes to stderr; without search, the summary goes to stdout.\n\
    N is a positive decimal byte budget, including discarded input; default 67108864.\n\
    Lines retain at most 4096 bytes. Use -- before a path beginning with '-'.";

#[derive(Debug)]
struct Options {
    path: OsString,
    query: Option<String>,
    limits: Limits,
}
#[derive(Debug)]
enum Command {
    Help,
    Run(Options),
}
fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}
fn parse_args(mut args: impl Iterator<Item = OsString>) -> io::Result<Command> {
    let mut query = None;
    let mut budget = None;
    while let Some(argument) = args.next() {
        if argument == "--help" && query.is_none() && budget.is_none() {
            return if args.next().is_none() {
                Ok(Command::Help)
            } else {
                Err(invalid("--help must be used alone"))
            };
        }
        if argument == "--contains" {
            if query.is_some() {
                return Err(invalid("--contains was supplied twice"));
            }
            let value = args
                .next()
                .ok_or_else(|| invalid("--contains needs a literal"))?;
            let value = value
                .into_string()
                .map_err(|_| invalid("query must be UTF-8"))?;
            if value.is_empty() || value.len() > 4096 || value.chars().any(char::is_control) {
                return Err(invalid(
                    "query must be 1..=4096 UTF-8 bytes without control characters",
                ));
            }
            query = Some(value);
        } else if argument == "--max-bytes" {
            if budget.is_some() {
                return Err(invalid("--max-bytes was supplied twice"));
            }
            let value = args
                .next()
                .ok_or_else(|| invalid("--max-bytes needs a positive integer"))?;
            let text = value
                .to_str()
                .ok_or_else(|| invalid("byte budget must be UTF-8 decimal digits"))?;
            if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
                return Err(invalid("byte budget must contain only decimal digits"));
            }
            budget = Some(
                text.parse::<u64>()
                    .ok()
                    .filter(|&n| n > 0)
                    .ok_or_else(|| invalid("byte budget must be 1..=18446744073709551615"))?,
            );
        } else {
            let path = if argument == "--" {
                args.next().ok_or_else(|| invalid("-- needs a file path"))?
            } else {
                if argument != "-" && argument.as_encoded_bytes().starts_with(b"-") {
                    return Err(invalid(
                        "unknown option; use -- before a path beginning with '-'",
                    ));
                }
                argument
            };
            if args.next().is_some() {
                return Err(invalid("expected one file path after the options"));
            }
            return Ok(Command::Run(Options {
                path,
                query,
                limits: Limits {
                    max_total_bytes: budget.unwrap_or(Limits::default().max_total_bytes),
                    ..Limits::default()
                },
            }));
        }
    }
    Err(invalid(USAGE))
}
fn write_summary(output: &mut impl Write, report: &Report) -> io::Result<()> {
    write!(
        output,
        "lines={} valid={} malformed={} oversized={} info={} warnings={} errors={}",
        report.lines,
        report.valid,
        report.malformed,
        report.oversized,
        report.info,
        report.warnings,
        report.errors
    )
}
fn process(reader: impl BufRead, options: &Options) -> io::Result<()> {
    let mut stdout = io::stdout().lock();
    if let Some(query) = &options.query {
        let report = search(reader, options.limits, query, &mut stdout)?;
        // Output failures, including a broken pipe, are errors. Do not print a
        // success summary before the matching records have been flushed.
        stdout.flush()?;
        let mut stderr = io::stderr().lock();
        write_summary(&mut stderr, &report.summary)?;
        writeln!(stderr, " matched={}", report.matched)?;
        stderr.flush()?;
    } else {
        let report = analyze(reader, options.limits)?;
        write_summary(&mut stdout, &report)?;
        writeln!(stdout)?;
        stdout.flush()?;
    }
    Ok(())
}
fn run() -> io::Result<()> {
    let options = match parse_args(std::env::args_os().skip(1))? {
        Command::Help => {
            let mut stdout = io::stdout().lock();
            writeln!(stdout, "{USAGE}")?;
            return stdout.flush();
        }
        Command::Run(options) => options,
    };
    if options.path == "-" {
        process(io::stdin().lock(), &options)
    } else {
        let file = File::open(&options.path).map_err(|error| {
            io::Error::new(error.kind(), format!("open {:?}: {error}", options.path))
        })?;
        process(BufReader::with_capacity(8192, file), &options)
    }
}
fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            // A failed diagnostic sink cannot repair the original failure.
            let _ = writeln!(io::stderr().lock(), "logs: {error}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn parse(args: &[&str]) -> io::Result<Command> {
        parse_args(args.iter().map(OsString::from))
    }
    #[test]
    fn argument_contract() {
        let Command::Run(options) =
            parse(&["--contains", "café", "--max-bytes", "1000000000", "-"]).unwrap()
        else {
            panic!("expected run")
        };
        assert_eq!(options.query.as_deref(), Some("café"));
        assert_eq!(options.limits.max_total_bytes, 1_000_000_000);
        assert_eq!(options.path, "-");
        assert!(matches!(parse(&["--help"]).unwrap(), Command::Help));
        assert!(matches!(
            parse(&["--", "--a-file"]).unwrap(),
            Command::Run(_)
        ));
        for arguments in [
            vec![],
            vec!["--contains", "", "-"],
            vec!["--contains"],
            vec!["--max-bytes", "0", "-"],
            vec!["--max-bytes", "-1", "-"],
            vec!["--max-bytes", "18446744073709551616", "-"],
            vec!["--max-bytes", "5", "--max-bytes", "6", "-"],
            vec!["--contains", "x", "--contains", "y", "-"],
            vec!["--unknown"],
            vec!["one", "two"],
            vec!["--"],
            vec!["--help", "-"],
        ] {
            assert!(parse(&arguments).is_err(), "{arguments:?}");
        }
    }
    #[cfg(unix)]
    #[test]
    fn native_path_is_preserved_but_non_unicode_query_is_rejected() {
        use std::os::unix::ffi::OsStringExt;
        let path = OsString::from_vec(vec![b'f', 0xff]);
        let Command::Run(options) = parse_args([path.clone()].into_iter()).unwrap() else {
            panic!("expected run")
        };
        assert_eq!(options.path, path);
        assert!(
            parse_args([OsString::from("--contains"), path, OsString::from("-")].into_iter())
                .is_err()
        );
    }
}
