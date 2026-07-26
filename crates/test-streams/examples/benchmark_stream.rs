//! Run with:
//!   cargo run -p test-streams --example benchmark_stream --features hotpath

use futures_util::stream::{self, StreamExt};
use std::time::Instant;

// Simple single-threaded stress test: wraps a long iterator stream with the
// `stream!` macro and drains it in a tight loop, so the measured time reflects
// per-item instrumentation overhead. Compare `--features hotpath` against a
// plain run.
fn main() {
    smol::block_on(async {
        let _guard = hotpath::HotpathGuardBuilder::new("main")
            .sections(vec![hotpath::Section::Streams])
            .build();

        let runs = bench_runs();
        let mut s = hotpath::stream!(
            stream::iter(0..runs).map(|v| {
                spin_1us();
                v
            }),
            label = "counter"
        );

        let start = Instant::now();
        while let Some(v) = s.next().await {
            spin_1us();
            std::hint::black_box(v);
        }
        let elapsed = start.elapsed();

        println!(
            "stream!: {runs} items in {elapsed:?} ({:.1} ns/op)",
            elapsed.as_nanos() as f64 / runs as f64
        );
    })
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
