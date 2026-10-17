use std::io::{self, Write};

fn main() -> io::Result<()> {
    let mut lab = p15_replication::Lab::default();
    lab.partition(0, 1, true);
    lab.partition(0, 2, true);
    // The fixed fixture has an online leader and an empty 1000-entry log.
    lab.propose(42)
        .expect("one fixed proposal fits the fresh online-leader fixture");
    lab.drain();
    let mut output = io::stdout().lock();
    writeln!(
        output,
        "partition: committed={} prefix_safe={}",
        lab.committed,
        lab.prefix_safe()
    )?;
    lab.partition(0, 1, false);
    lab.resend();
    lab.drain();
    writeln!(
        output,
        "healed: committed={} prefix_safe={}",
        lab.committed,
        lab.prefix_safe()
    )?;
    output.flush()
}
