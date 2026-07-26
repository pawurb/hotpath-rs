//! Run with:
//!   cargo run -p test-channels-ftc --example benchmark_channel_ftc --features hotpath

use futures_util::stream::StreamExt;
use std::time::{Duration, Instant};

// Single-threaded stress test comparing channel instrumentation overhead in one run: an
// uninstrumented baseline (raw channel, no macro) and the `proxy = true` forwarder. Each is
// hammered in a tight uncontended send/recv loop, so the delta vs baseline isolates the
// per-send/recv instrumentation cost. futures_channel has no wrap implementation, so only
// the forwarder mode is available. Run with `--features hotpath`; iteration count via
// `HOTPATH_BENCH_RUNS`.
fn main() {
    smol::block_on(async {
        let _guard = hotpath::HotpathGuardBuilder::new("main")
            .sections(vec![hotpath::Section::Channels])
            .build();

        let runs = bench_runs();

        macro_rules! phase {
            ($ch:expr) => {{
                let (tx, mut rx) = $ch;
                let start = Instant::now();
                for i in 0..runs {
                    tx.unbounded_send(i).unwrap();
                    spin_1us();
                    rx.next().await.unwrap();
                }
                start.elapsed()
            }};
        }

        let baseline = phase!(futures_channel::mpsc::unbounded::<u64>());
        let proxy = phase!(hotpath::channel!(
            futures_channel::mpsc::unbounded::<u64>(),
            proxy = true,
            label = "proxy"
        ));

        report(
            "futures",
            runs,
            &[("baseline (raw)", baseline), ("proxy = true", proxy)],
        );
    })
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
