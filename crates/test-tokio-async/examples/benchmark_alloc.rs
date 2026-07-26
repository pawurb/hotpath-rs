//! Run with:
//!   cargo run -p test-tokio-async --example benchmark_alloc --features hotpath,hotpath-alloc

use std::thread;
use std::time::{Duration, Instant};

// Multi-threaded stress test comparing function instrumentation overhead in one run: an
// uninstrumented baseline (`raw_alloc`) and the `#[hotpath::measure]` instrumented version
// (`instrumented_alloc`). Each is hammered across `HOTPATH_ALLOC_NUM_THREADS` threads, so the
// delta vs baseline isolates the per-call instrumentation cost. Run with
// `--features hotpath,hotpath-alloc`. Iteration count via `HOTPATH_BENCH_RUNS`.
fn main() {
    let num_threads = num_threads();
    let runs_per_thread = bench_runs();
    let total = runs_per_thread * num_threads as u64;

    // Warm the global allocator arenas and CPU caches so neither mode eats one-time
    // startup cost. Both bodies run, but the guard isn't built yet so `#[measure]` is
    // inert here and these calls are discarded from the report. Without this the
    // first-timed mode looks slower than the second, which can make the instrumented
    // run appear faster than the raw baseline.
    bench(num_threads, runs_per_thread, raw_alloc);
    bench(num_threads, runs_per_thread, instrumented_alloc);

    // Build the guard after warmup: guard-init cost and the discarded warmup calls
    // stay out of the timed section and the final report.
    let _guard = hotpath::HotpathGuardBuilder::new("main").build();

    // Best-of-N, interleaved, so scheduler/allocator noise doesn't bias one mode.
    let mut baseline = Duration::MAX;
    let mut instrumented = Duration::MAX;
    for _ in 0..3 {
        baseline = baseline.min(bench(num_threads, runs_per_thread, raw_alloc));
        instrumented = instrumented.min(bench(num_threads, runs_per_thread, instrumented_alloc));
    }

    report("alloc", total, baseline, instrumented);
}

fn bench(num_threads: usize, runs_per_thread: u64, f: fn()) -> Duration {
    let start = Instant::now();
    let handles: Vec<_> = (0..num_threads)
        .map(|_| {
            thread::spawn(move || {
                for _ in 0..runs_per_thread {
                    f();
                }
            })
        })
        .collect();

    for handle in handles {
        handle.join().unwrap();
    }
    start.elapsed()
}

fn report(name: &str, total: u64, baseline: Duration, instrumented: Duration) {
    let per = |d: Duration| d.as_nanos() as f64 / total as f64;
    let b = per(baseline);
    let ins = per(instrumented);
    println!("\n{name}: {total} calls per mode");
    println!("  baseline (raw)  {b:>8.1} ns/op");
    println!(
        "  instrumented    {ins:>8.1} ns/op  ({:+.1} ns/op vs baseline)",
        ins - b
    );
}

#[hotpath::measure]
fn instrumented_alloc() {
    for _ in 0..1000 {
        let vec = vec![1u8; 128];
        std::hint::black_box(vec);
    }
}

fn raw_alloc() {
    for _ in 0..1000 {
        let vec = vec![1u8; 128];
        std::hint::black_box(vec);
    }
}

fn num_threads() -> usize {
    std::env::var("HOTPATH_ALLOC_NUM_THREADS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(3)
}

fn bench_runs() -> u64 {
    std::env::var("HOTPATH_BENCH_RUNS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(10_000)
}
