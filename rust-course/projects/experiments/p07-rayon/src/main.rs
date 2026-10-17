use indexer_parallel_tools::{fixture, rayon_pool, scoped_threads, serial};
use std::process::ExitCode;

fn run() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::args_os().len() != 1 {
        return Err("this fixed-fixture demonstration accepts no arguments".into());
    }
    let inputs = fixture(32, 32 * 1024)?;
    let expected = serial(&inputs)?;
    let threads = scoped_threads(&inputs, 4)?;
    let parallel = rayon_pool(&inputs, 4)?;
    if threads != expected || parallel.fingerprints != expected {
        return Err("ordered strategy outputs differ; comparison rejected".into());
    }
    println!(
        "files={} input_bytes={} workers={} equal_ordered_outputs=true",
        inputs.len(),
        inputs.iter().map(Vec::len).sum::<usize>(),
        parallel.pool_threads
    );
    println!("fingerprints={expected:?}");
    Ok(())
}
fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("parallel tools: {error}");
            ExitCode::FAILURE
        }
    }
}
