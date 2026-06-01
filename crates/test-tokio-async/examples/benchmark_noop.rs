use std::time::Instant;

#[hotpath::measure]
fn noop() {}

#[hotpath::main]
fn main() {
    let runs = std::env::var("HOTPATH_BENCHMARK_NOOP_RUNS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100_000);

    let start = Instant::now();
    for _ in 0..runs {
        noop();
    }
    let elapsed = start.elapsed();

    println!(
        "noop: {runs} calls in {elapsed:?} ({:.1} ns/op)",
        elapsed.as_nanos() as f64 / runs as f64
    );
}
