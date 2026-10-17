use checkpoint_p01_calculator::add_text;
use std::io::{self, Write};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let sum = add_text("7", "5")?;
    let mut output = io::stdout().lock();
    writeln!(output, "{sum}")?;
    output.flush()?;
    Ok(())
}
