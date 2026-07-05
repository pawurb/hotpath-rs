use std::time::Instant;

// Same single-threaded stress test as `benchmark_channel_tokio`.
// Wrap mode instruments the channel endpoints directly instead of relaying every message
// through a forwarder task and a second channel, so this measures the per-send/recv
// overhead of the wrapped endpoints. Compare against `benchmark_channel_tokio` to see the
// wrap-vs-forwarder cost difference.
#[tokio::main]
async fn main() {
    let _guard = hotpath::HotpathGuardBuilder::new("main")
        .sections(vec![hotpath::Section::Channels])
        .build();

    let runs = bench_runs();
    let (tx, mut rx) = hotpath::channel!(
        tokio::sync::mpsc::unbounded_channel::<u64>(),
        label = "counter"
    );

    let start = Instant::now();
    for i in 0..runs {
        tx.send(i).unwrap();
        spin_1us();
        rx.recv().await.unwrap();
    }
    let elapsed = start.elapsed();

    println!(
        "tokio wrap channel: {runs} send/recv cycles in {elapsed:?} ({:.1} ns/op)",
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
