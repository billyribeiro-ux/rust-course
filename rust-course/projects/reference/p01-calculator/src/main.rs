use std::{
    io::{self, Write},
    process::ExitCode,
};
fn run() -> io::Result<ExitCode> {
    let args = std::env::args_os()
        .skip(1)
        .map(|value| value.into_string())
        .collect::<Result<Vec<_>, _>>();
    let mut stdout = io::stdout().lock();
    let mut stderr = io::stderr().lock();
    let Ok(args) = args else {
        writeln!(stderr, "calculator: arguments must be valid UTF-8 text")?;
        return Ok(ExitCode::from(2));
    };
    let [left, op, right] = args.as_slice() else {
        writeln!(
            stderr,
            "usage: workshop-calculator <integer[m|s]> <+|-|*|/> <integer[m|s]>"
        )?;
        writeln!(
            stderr,
            "       workshop-calculator convert <finite-number><mm|cm|m|km> <mm|cm|m|km>"
        )?;
        return Ok(ExitCode::from(2));
    };
    if left == "convert" {
        return match workshop_calculator::conversion::convert(op, right) {
            Ok(length) => {
                writeln!(
                    stdout,
                    "approximately {}{}",
                    length.value(),
                    length.unit().symbol()
                )?;
                stdout.flush()?;
                Ok(ExitCode::SUCCESS)
            }
            Err(error) => {
                writeln!(stderr, "conversion: {error}")?;
                Ok(ExitCode::FAILURE)
            }
        };
    }
    match workshop_calculator::calculate(left, op, right) {
        Ok(value) => {
            writeln!(stdout, "{value}")?;
            stdout.flush()?;
            Ok(ExitCode::SUCCESS)
        }
        Err(error) => {
            writeln!(stderr, "calculator: {error}")?;
            Ok(ExitCode::FAILURE)
        }
    }
}
fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(error) => {
            // Diagnostic output is best effort; its failure cannot restore stdout.
            let _ = writeln!(io::stderr().lock(), "calculator output: {error}");
            ExitCode::FAILURE
        }
    }
}
