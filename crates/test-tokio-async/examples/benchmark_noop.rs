use std::time::{Duration, Instant};

// Single-threaded stress test comparing function instrumentation overhead in one run: an
// uninstrumented baseline (`raw_noop`) and the `#[hotpath::measure]` instrumented version
// (`instrumented_noop`). Each is hammered in a tight loop, so the delta vs baseline isolates
// the per-call instrumentation cost. Run with `--features hotpath` (without it the macro is a
// no-op and both modes are the raw function). Iteration count via `HOTPATH_BENCH_RUNS`.
#[hotpath::main]
fn main() {
    let runs = bench_runs();

    let baseline = bench(runs, raw_noop);
    let instrumented = bench(runs, instrumented_noop);

    report("noop", runs, baseline, instrumented);
}

fn bench(runs: u64, f: fn()) -> Duration {
    let start = Instant::now();
    for _ in 0..runs {
        f();
    }
    start.elapsed()
}

fn report(name: &str, runs: u64, baseline: Duration, instrumented: Duration) {
    let per = |d: Duration| d.as_nanos() as f64 / runs as f64;
    let b = per(baseline);
    let ins = per(instrumented);
    println!("\n{name}: {runs} calls per mode");
    println!("  baseline (raw)  {b:>8.1} ns/op");
    println!(
        "  instrumented    {ins:>8.1} ns/op  ({:+.1} ns/op vs baseline)",
        ins - b
    );
}

#[hotpath::measure]
fn instrumented_noop() {
    let a = 0;
    std::hint::black_box(a);
    spin_1us();
}

fn raw_noop() {
    let a = 0;
    std::hint::black_box(a);
    spin_1us();
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
