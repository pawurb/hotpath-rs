//! Run with:
//!   cargo run -p test-rw-lock-tokio --example benchmark_rw_lock_tokio --features hotpath

use std::time::{Duration, Instant};

// Overhead benchmark: raw lock vs `hotpath::rw_lock!`, a write loop then a read loop.
// Without `--features hotpath` both modes are raw. Iterations via `HOTPATH_BENCH_RUNS`.
#[tokio::main]
async fn main() {
    let _guard = hotpath::HotpathGuardBuilder::new("main")
        .sections(vec![hotpath::Section::RwLocks])
        .build();

    let runs = bench_runs();

    macro_rules! bench {
        ($lock:expr) => {{
            let lock = $lock;
            let start = Instant::now();
            for _ in 0..runs {
                let mut w = lock.write().await;
                *w += 1;
                spin_1us();
            }
            let write_elapsed = start.elapsed();

            let start = Instant::now();
            let mut acc = 0u64;
            for _ in 0..runs {
                let r = lock.read().await;
                acc = acc.wrapping_add(*r);
                spin_1us();
            }
            std::hint::black_box(acc);
            (write_elapsed, start.elapsed())
        }};
    }

    let (base_w, base_r) = bench!(tokio::sync::RwLock::new(0u64));
    let (ins_w, ins_r) = bench!(hotpath::rw_lock!(
        tokio::sync::RwLock::new(0u64),
        label = "counter"
    ));

    report("tokio RwLock writes", runs, base_w, ins_w);
    report("tokio RwLock reads", runs, base_r, ins_r);
}

fn report(name: &str, runs: u64, baseline: Duration, instrumented: Duration) {
    let per = |d: Duration| d.as_nanos() as f64 / runs as f64;
    let b = per(baseline);
    let ins = per(instrumented);
    println!("\n{name}: {runs} cycles per mode");
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
