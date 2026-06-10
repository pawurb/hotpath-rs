use std::time::Instant as StdInstant;

use hotpath::instant::Instant;

fn main() {
    let runs: u32 = std::env::var("HOTPATH_BENCH_RUNS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(10_000_000);

    let start = StdInstant::now();
    for _ in 0..runs {
        std::hint::black_box(Instant::now());
    }
    let custom_ns = start.elapsed().as_nanos() as f64 / f64::from(runs);

    let start = StdInstant::now();
    for _ in 0..runs {
        std::hint::black_box(StdInstant::now());
    }
    let std_ns = start.elapsed().as_nanos() as f64 / f64::from(runs);

    println!("custom Instant::now(): {custom_ns:.1} ns/call");
    println!("std    Instant::now(): {std_ns:.1} ns/call");
}
