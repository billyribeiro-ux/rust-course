use p06_kv::{MAX_KEY, MAX_VALUE, Store};
use std::{
    ffi::OsStr,
    io::{self, Write},
    path::PathBuf,
    process::ExitCode,
};

enum Command<'a> {
    Put(&'a [u8], &'a [u8]),
    Get(&'a [u8]),
    Delete(&'a [u8]),
    #[cfg(unix)]
    Compact,
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}

fn key(input: &OsStr) -> io::Result<&[u8]> {
    let text = input
        .to_str()
        .ok_or_else(|| invalid("key must be valid UTF-8"))?;
    if text.is_empty() || text.len() > MAX_KEY {
        return Err(invalid("key must contain 1..=4096 UTF-8 bytes"));
    }
    Ok(text.as_bytes())
}

fn run() -> io::Result<()> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let [path, command, rest @ ..] = args.as_slice() else {
        return Err(invalid(
            "usage: p06-kv PATH put KEY VALUE | get KEY | delete KEY | compact (Unix)",
        ));
    };
    if path.is_empty() {
        return Err(invalid("path must not be empty"));
    }
    let path = PathBuf::from(path);
    let command = command
        .to_str()
        .ok_or_else(|| invalid("command must be valid UTF-8"))?;
    // Validate the whole request before open: open may create a file or repair a tail.
    let command = match (command, rest) {
        ("put", [k, value]) => {
            let k = key(k)?;
            let value = value
                .to_str()
                .ok_or_else(|| invalid("value must be valid UTF-8"))?;
            if value.len() > MAX_VALUE {
                return Err(invalid("value exceeds 1048576 UTF-8 bytes"));
            }
            Command::Put(k, value.as_bytes())
        }
        ("get", [k]) => Command::Get(key(k)?),
        ("delete", [k]) => Command::Delete(key(k)?),
        #[cfg(unix)]
        ("compact", []) => Command::Compact,
        _ => return Err(invalid("invalid command or argument count")),
    };
    let mut store = Store::open(path)?;
    if store.recovered_torn_tail {
        // Recovery already happened. A diagnostic failure cannot undo it.
        let _ = writeln!(io::stderr().lock(), "recovered an incomplete final record");
    }
    match command {
        Command::Put(key, value) => store.put(key, value)?,
        Command::Get(key) => {
            let mut output = io::stdout().lock();
            match store.get(key) {
                Some(value) => writeln!(output, "{}", String::from_utf8_lossy(value))?,
                None => writeln!(output, "(missing)")?,
            }
            output.flush()?;
        }
        Command::Delete(key) => store.delete(key)?,
        #[cfg(unix)]
        Command::Compact => store.compact()?,
    }
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            let _ = writeln!(io::stderr().lock(), "kv: {error}");
            if error.kind() == io::ErrorKind::InvalidInput {
                ExitCode::from(2)
            } else {
                ExitCode::FAILURE
            }
        }
    }
}
