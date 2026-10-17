use std::io::{self, Write};

fn main() -> io::Result<()> {
    let sum = 0.1_f64 + 0.2;
    let mut output = io::stdout().lock();
    writeln!(output, "0.1 + 0.2 = {sum:.17}")?;
    writeln!(output, "equal to 0.3: {}", sum == 0.3)?;
    output.flush()
}
