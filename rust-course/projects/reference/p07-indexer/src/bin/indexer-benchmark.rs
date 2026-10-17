//! Warm-cache comparison of the same bounded indexer with one and four workers.
//! Fixture generation is outside measurement and leaves an inspectable corpus.
use p07_indexer::{Config, Report, Status};
use std::{
    fs::{self, OpenOptions},
    hint::black_box,
    io::{self, Write},
    path::{Path, PathBuf},
    sync::{Arc, atomic::AtomicBool},
    time::{Instant, SystemTime, UNIX_EPOCH},
};
const FILES: usize = 64;
const BYTES_PER_FILE: usize = 128 * 1024;
const SAMPLES: usize = 7;
const WARMUPS_PER_VARIANT: usize = 2;
fn create_fixture(parent: &Path) -> io::Result<(PathBuf, Vec<PathBuf>)> {
    fs::create_dir_all(parent)?;
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(io::Error::other)?
        .as_nanos();
    let directory = parent.join(format!("indexer-v1-{}-{stamp}", std::process::id()));
    fs::create_dir(&directory)?;
    let mut paths = Vec::with_capacity(FILES);
    let mut payload = Vec::with_capacity(BYTES_PER_FILE);
    for index in 0..FILES {
        payload.clear();
        let line = format!(
            "file={index:02} alpha beta gamma delta 0123456789 bounded deterministic fixture\n"
        );
        while payload.len() < BYTES_PER_FILE {
            let remaining = BYTES_PER_FILE - payload.len();
            payload.extend_from_slice(&line.as_bytes()[..remaining.min(line.len())]);
        }
        let path = directory.join(format!("file-{index:02}.txt"));
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?;
        file.write_all(&payload)?;
        file.sync_all()?;
        paths.push(path);
    }
    // Input order differs from output order; every trial includes result sorting.
    paths.reverse();
    Ok((fs::canonicalize(directory)?, paths))
}
fn run_variant(paths: &[PathBuf], workers: usize) -> io::Result<Report> {
    p07_indexer::index(
        paths.to_vec(),
        Config {
            workers,
            queue: 8,
            max_files: FILES,
            max_bytes: BYTES_PER_FILE as u64,
        },
        Arc::new(AtomicBool::new(false)),
    )
}
fn require_same(actual: &Report, expected: &Report) -> io::Result<()> {
    // Peak concurrency is instrumentation that naturally differs between variants.
    // Compare every deterministic result, including every field of each Entry.
    if actual.entries != expected.entries
        || actual.submitted != expected.submitted
        || actual.cancelled != expected.cancelled
    {
        return Err(io::Error::other(
            "complete deterministic index results differ",
        ));
    }
    Ok(())
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arguments: Vec<_> = std::env::args_os().skip(1).collect();
    let parent = match arguments.as_slice() {
        [] => Path::new(".course-cache/benchmark-fixtures"),
        [path] => Path::new(path),
        _ => return Err("usage: indexer-benchmark [fixture_parent_directory]".into()),
    };
    if cfg!(debug_assertions) {
        return Err("comparative measurement requires cargo run --release".into());
    }
    let (directory, paths) = create_fixture(parent)?;
    let expected = black_box(run_variant(&paths, 1)?);
    if expected.cancelled
        || expected.submitted != FILES
        || expected.entries.len() != FILES
        || expected
            .entries
            .iter()
            .any(|entry| entry.status != Status::Indexed || entry.bytes != BYTES_PER_FILE as u64)
    {
        return Err("fixture was not fully and successfully indexed".into());
    }
    for warmup in 0..WARMUPS_PER_VARIANT {
        for workers in if warmup % 2 == 0 { [1, 4] } else { [4, 1] } {
            require_same(&black_box(run_variant(&paths, workers)?), &expected)?;
        }
    }
    println!(
        "benchmark=indexer profile=release files={FILES} bytes_per_file={BYTES_PER_FILE} total_bytes={} samples_per_variant={SAMPLES} warmups_per_variant={WARMUPS_PER_VARIANT} untimed_reference_runs=1 queue=8",
        FILES * BYTES_PER_FILE
    );
    println!("fixture_dir={}", directory.display());
    println!(
        "fixture_generator=indexer-v1 source=projects/reference/p07-indexer/src/bin/indexer-benchmark.rs fixture_creation_timed=false cache=warm_after_baseline_and_warmups"
    );
    println!(
        "timed=path_clone_thread_start_file_open_read_count_collection_sort excluded=fixture_generation_full_result_comparison_output"
    );
    println!(
        "sample\tposition\tvariant\telapsed_ns\tsubmitted\tentries\tpeak_workers\tfull_output_equal"
    );
    for sample in 0..SAMPLES {
        for (position, workers) in (if sample % 2 == 0 { [1, 4] } else { [4, 1] })
            .into_iter()
            .enumerate()
        {
            let start = Instant::now();
            let report = black_box(run_variant(black_box(&paths), workers)?);
            let elapsed = start.elapsed().as_nanos();
            require_same(&report, &expected)?;
            println!(
                "{}\t{}\tworkers{workers}\t{elapsed}\t{}\t{}\t{}\ttrue",
                sample + 1,
                position + 1,
                report.submitted,
                report.entries.len(),
                report.peak_workers
            );
        }
    }
    println!(
        "limit=local_warm_cache_observation_only cold_disk=false randomization=false order=alternating_pairs fixture_retained=true"
    );
    Ok(())
}
