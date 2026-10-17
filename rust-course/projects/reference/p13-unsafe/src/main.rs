use std::io::{self, Write};

fn main() -> io::Result<()> {
    let mut slots = p13_unsafe::FixedSlots::<String, 2>::new();
    // These fixed demonstration pushes cannot fill capacity before the second value.
    // A failure would violate this program's own construction invariant, not user input.
    slots
        .push("ownership".into())
        .expect("new capacity-two slots have room for the first value");
    slots
        .push("invariants".into())
        .expect("capacity-two slots have room for the second value");
    let mut output = io::stdout().lock();
    writeln!(output, "live={:?}", slots.as_slice())?;
    writeln!(
        output,
        "full returns ownership: {:?}",
        slots.push("third".into())
    )?;
    writeln!(output, "pop={:?}", slots.pop())?;
    writeln!(output, "remaining={:?}", slots.as_slice())?;
    output.flush()
}
