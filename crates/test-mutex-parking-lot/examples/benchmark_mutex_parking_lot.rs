//! Run with:
//!   cargo run -p test-mutex-parking-lot --example benchmark_mutex_parking_lot --features hotpath

use std::time::{Duration, Instant};

// Single-threaded stress test comparing mutex instrumentation overhead in one run: an
// uninstrumented baseline (raw mutex) and the `hotpath::mutex!` instrumented version. Each
// is hammered in a tight uncontended lock loop, so the delta vs baseline isolates the
// per-lock instrumentation cost. Run with `--features hotpath` (without it the macro is a
// no-op and both modes are the raw mutex). Iteration count via `HOTPATH_BENCH_RUNS`.
fn main() {
    let _guard = hotpath::HotpathGuardBuilder::new("main")
        .sections(vec![hotpath::Section::Mutexes])
        .build();

    let runs = bench_runs();

    macro_rules! bench {
        ($lock:expr) => {{
            let lock = $lock;
            let start = Instant::now();
            for _ in 0..runs {
                let mut v = lock.lock();
                *v += 1;
                spin_1us();
            }
            start.elapsed()
        }};
    }

    let baseline = bench!(parking_lot::Mutex::new(0u64));
    let instrumented = bench!(hotpath::mutex!(
        parking_lot::Mutex::new(0u64),
        label = "counter"
    ));

    report("parking_lot Mutex", runs, baseline, instrumented);
}

fn report(name: &str, runs: u64, baseline: Duration, instrumented: Duration) {
    let per = |d: Duration| d.as_nanos() as f64 / runs as f64;
    let b = per(baseline);
    let ins = per(instrumented);
    println!("\n{name}: {runs} lock cycles per mode");
    println!("  baseline (raw)  {b:>8.1} ns/op");
    println!(
        "  instrumented    {ins:>8.1} ns/op  ({:+.1} ns/op vs baseline)",
        ins - b
    );
}

#[inline(never)]
fn spin_1us() {
    let start = Instant::now();
    while start.elapsed().as_nanos() < 1000 {
        std::hint::spin_loop();
    }
}

fn bench_runs() -> u64 {
    std::env::var("HOTPATH_BENCH_RUNS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100_000)
}
