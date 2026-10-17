use std::io::{self, Write};

fn main() -> io::Result<()> {
    let mut output = io::stdout().lock();
    writeln!(output, "Hello, Rust workshop!")?;
    output.flush()
}
