use std::{
    fs::File,
    io::{self, Read, Write},
    process::ExitCode,
};
use typed_config::{Config, Layer};
fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let mut file = Layer::default();
    let mut cli = Layer::default();
    let mut index = 0;
    while index < args.len() {
        match args[index].to_str().ok_or("option must be valid UTF-8")? {
            "--file" => {
                index += 1;
                let path = args.get(index).ok_or("--file needs a path")?;
                let mut text = String::new();
                File::open(path)?.take(16_385).read_to_string(&mut text)?;
                file = Layer::parse(&text)?;
            }
            "--set" => {
                index += 1;
                let entry = args
                    .get(index)
                    .ok_or("--set needs key=value")?
                    .to_str()
                    .ok_or("--set value must be valid UTF-8")?;
                let (key, value) = entry.split_once('=').ok_or("--set needs key=value")?;
                cli.insert(key, value)?;
            }
            _ => return Err("usage: typed-config [--file <path>] [--set key=value]...".into()),
        }
        index += 1;
    }
    let pairs = std::env::vars_os()
        .filter_map(|(k, v)| Some((k.into_string().ok()?, v.into_string().ok()?)));
    let config = Config::resolve([file, Layer::environment(pairs)?, cli])?;
    let mut stdout = io::stdout().lock();
    writeln!(stdout, "{config:?}")?;
    stdout.flush()?;
    Ok(())
}
fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            let _ = writeln!(io::stderr().lock(), "config: {e}");
            ExitCode::FAILURE
        }
    }
}
