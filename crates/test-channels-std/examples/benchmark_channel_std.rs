//! Run with:
//!   cargo run -p test-channels-std --example benchmark_channel_std --features hotpath

use std::time::{Duration, Instant};

// Single-threaded stress test comparing channel instrumentation overhead in one run: an
// uninstrumented baseline (raw channel, no macro), the `proxy = true` forwarder, and the
// default wrap mode. Each is hammered in a tight uncontended send/recv loop, so the delta
// vs baseline isolates per-send/recv instrumentation cost. Run with `--features hotpath`
// (without it every mode is the raw channel). Iteration count via `HOTPATH_BENCH_RUNS`.
fn main() {
    let _guard = hotpath::HotpathGuardBuilder::new("main")
        .sections(vec![hotpath::Section::Channels])
        .build();

    let runs = bench_runs();

    macro_rules! phase {
        ($ch:expr) => {{
            let (tx, rx) = $ch;
            let start = Instant::now();
            for i in 0..runs {
                tx.send(i).unwrap();
                spin_1us();
                rx.recv().unwrap();
            }
            start.elapsed()
        }};
    }

    let baseline = phase!(std::sync::mpsc::channel::<u64>());
    let proxy = phase!(hotpath::channel!(
        std::sync::mpsc::channel::<u64>(),
        proxy = true,
        label = "proxy"
    ));
    let wrap = phase!(hotpath::channel!(
        std::sync::mpsc::channel::<u64>(),
        label = "wrap"
    ));

    report(
        "std",
        runs,
        &[
            ("baseline (raw)", baseline),
            ("wrap (default)", wrap),
            ("proxy = true", proxy),
        ],
    );
}

fn report(name: &str, runs: u64, rows: &[(&str, Duration)]) {
    let per = |d: Duration| d.as_nanos() as f64 / runs as f64;
    let baseline = per(rows[0].1);
    println!("\n{name} channel: {runs} send/recv cycles per mode");
    for (i, (label, elapsed)) in rows.iter().enumerate() {
        let p = per(*elapsed);
        if i == 0 {
            println!("  {label:<15} {p:>8.1} ns/op");
        } else {
            println!(
                "  {label:<15} {p:>8.1} ns/op  ({:+.1} ns/op vs baseline)",
                p - baseline
            );
        }
    }
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
