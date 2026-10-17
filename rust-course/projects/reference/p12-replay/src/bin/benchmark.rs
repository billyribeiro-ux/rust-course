//! Compare immediate event application with a bounded, reusable event batch.
//! Both timed regions include synthetic generation and complete replay work.
use p12_replay::{Event, Replay};
use std::{hint::black_box, io, time::Instant};
const BATCH_EVENTS: usize = 1024;
const SAMPLES: usize = 7;
const WARMUPS_PER_VARIANT: usize = 2;
fn batched(count: usize) -> Result<Replay, p12_replay::Error> {
    let mut source = p12_replay::synthetic(count)?;
    let mut replay = Replay::new(4096)?;
    let mut batch: Vec<Event> = Vec::with_capacity(BATCH_EVENTS);
    loop {
        // The iterator is lazy; no more than BATCH_EVENTS events are retained.
        batch.extend(source.by_ref().take(BATCH_EVENTS));
        if batch.is_empty() {
            return Ok(replay);
        }
        for event in batch.drain(..) {
            replay.apply(event)?;
        }
        // drain preserves allocation capacity for the next bounded batch.
    }
}
fn run_variant(name: &str, count: usize) -> Result<Replay, p12_replay::Error> {
    match name {
        "stream" => p12_replay::run(count),
        "batch1024" => batched(count),
        _ => unreachable!("variant names come from the fixed internal pair"),
    }
}
fn require_same(actual: &Replay, expected: &Replay) -> io::Result<()> {
    if actual != expected {
        return Err(io::Error::other("complete replay states differ"));
    }
    Ok(())
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arguments: Vec<_> = std::env::args_os().skip(1).collect();
    let count = match arguments.as_slice() {
        [] => 1_000_003,
        [text] => text
            .to_str()
            .ok_or("count must be UTF-8 integer text")?
            .parse::<usize>()?,
        _ => return Err("usage: benchmark [event_count 1..=10000000]".into()),
    };
    if !(1..=10_000_000).contains(&count) {
        return Err("benchmark count must be in 1..=10000000".into());
    }
    if cfg!(debug_assertions) {
        return Err("comparative measurement requires cargo run --release".into());
    }
    let expected = black_box(p12_replay::run(count)?);
    for warmup in 0..WARMUPS_PER_VARIANT {
        let order = if warmup % 2 == 0 {
            ["stream", "batch1024"]
        } else {
            ["batch1024", "stream"]
        };
        for variant in order {
            require_same(
                &black_box(run_variant(variant, black_box(count))?),
                &expected,
            )?;
        }
    }
    println!(
        "benchmark=replay profile=release events={count} samples_per_variant={SAMPLES} warmups_per_variant={WARMUPS_PER_VARIANT} untimed_reference_runs=1 batch_limit={BATCH_EVENTS}"
    );
    println!(
        "timed=generation_and_replay excluded=state_comparison_output includes_batch_allocation=true live_orders_peak=2"
    );
    println!(
        "sample\tposition\tvariant\telapsed_ns\taccepted\topen_orders\tdigest\tfull_state_equal"
    );
    for sample in 0..SAMPLES {
        let order = if sample % 2 == 0 {
            ["stream", "batch1024"]
        } else {
            ["batch1024", "stream"]
        };
        for (position, variant) in order.into_iter().enumerate() {
            let start = Instant::now();
            let state = black_box(run_variant(variant, black_box(count))?);
            let elapsed = start.elapsed().as_nanos();
            require_same(&state, &expected)?;
            println!(
                "{}\t{}\t{variant}\t{elapsed}\t{}\t{}\t{:016x}\ttrue",
                sample + 1,
                position + 1,
                state.accepted(),
                state.orders().len(),
                state.digest()
            );
        }
    }
    println!(
        "limit=local_observation_only no_disk_input=true randomization=false order=alternating_pairs"
    );
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn batch_boundaries_preserve_complete_state() {
        for count in [0, 1, 2, 3, 4, 1023, 1024, 1025, 2049, 10_003] {
            assert_eq!(
                batched(count).unwrap(),
                p12_replay::run(count).unwrap(),
                "count={count}"
            );
        }
    }
}
