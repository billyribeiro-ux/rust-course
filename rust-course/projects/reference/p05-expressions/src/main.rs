use std::{
    io::{self, Write},
    process::ExitCode,
};
fn run() -> io::Result<ExitCode> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let mut stderr = io::stderr().lock();
    let [source] = args.as_slice() else {
        writeln!(stderr, "usage: expression-language '<expression>'")?;
        return Ok(ExitCode::from(2));
    };
    let Some(source) = source.to_str() else {
        writeln!(stderr, "expression: source must be valid UTF-8 text")?;
        return Ok(ExitCode::from(2));
    };
    match expression_language::evaluate(source) {
        Ok(value) => {
            let mut stdout = io::stdout().lock();
            writeln!(stdout, "{value}")?;
            stdout.flush()?;
            Ok(ExitCode::SUCCESS)
        }
        Err(error) => {
            writeln!(stderr, "{error}\n{source}")?;
            Ok(ExitCode::FAILURE)
        }
    }
}
fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(error) => {
            // A closed diagnostic stream must not turn a routine I/O error into a panic.
            let _ = writeln!(io::stderr().lock(), "expression output: {error}");
            ExitCode::FAILURE
        }
    }
}
