//! Run with:
//!   cargo run -p test-futures --example benchmark_future --features hotpath

use hotpath::future;
use std::time::Instant;

// Simple single-threaded stress test: wraps a trivial ready future with the
// `future!` macro in a tight loop, so the measured time reflects per-future
// instrumentation overhead. Compare `--features hotpath` against a plain run.
#[tokio::main]
async fn main() {
    let _guard = hotpath::HotpathGuardBuilder::new("main")
        .sections(vec![hotpath::Section::Futures])
        .build();

    let runs = bench_runs();

    let start = Instant::now();
    for i in 0..runs {
        let v = future!(
            async move {
                spin_1us();
                i
            },
            label = "counter"
        )
        .await;
        spin_1us();
        std::hint::black_box(v);
    }
    let elapsed = start.elapsed();

    println!(
        "future!: {runs} polls in {elapsed:?} ({:.1} ns/op)",
        elapsed.as_nanos() as f64 / runs as f64
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
