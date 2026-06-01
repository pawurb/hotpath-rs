use std::time::Instant;

// Simple single-threaded stress test: hammers a single instrumented RwLock in a
// tight loop with no contention, so the measured time reflects per-lock
// instrumentation overhead. Compare `--features hotpath` against a plain run.
fn main() {
    let _guard = hotpath::HotpathGuardBuilder::new("main")
        .sections(vec![hotpath::Section::RwLocks])
        .build();

    let runs = bench_runs();
    let lock = hotpath::rw_lock!(parking_lot::RwLock::new(0u64), label = "counter");

    let start = Instant::now();
    for _ in 0..runs {
        let mut w = lock.write();
        *w += 1;
    }
    let write_elapsed = start.elapsed();

    let start = Instant::now();
    let mut acc = 0u64;
    for _ in 0..runs {
        let r = lock.read();
        acc = acc.wrapping_add(*r);
    }
    let read_elapsed = start.elapsed();

    println!(
        "parking_lot RwLock writes: {runs} in {write_elapsed:?} ({:.1} ns/op)",
        write_elapsed.as_nanos() as f64 / runs as f64
    );
    println!(
        "parking_lot RwLock reads:  {runs} in {read_elapsed:?} ({:.1} ns/op)",
        read_elapsed.as_nanos() as f64 / runs as f64
    );
    println!("Final value: {}, read acc: {acc}", *lock.read());
}

fn bench_runs() -> u64 {
    std::env::var("HOTPATH_LOCK_BENCH_RUNS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1_000_000)
}
