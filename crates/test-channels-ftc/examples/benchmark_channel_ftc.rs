use futures_util::stream::StreamExt;
use std::time::Instant;

// Simple single-threaded stress test: hammers a single instrumented channel in
// a tight loop with no contention, so the measured time reflects per-send/recv
// instrumentation overhead. Compare `--features hotpath` against a plain run.
fn main() {
    smol::block_on(async {
        let _guard = hotpath::HotpathGuardBuilder::new("main")
            .sections(vec![hotpath::Section::Channels])
            .build();

        let runs = bench_runs();
        let (tx, mut rx) = hotpath::channel!(
            futures_channel::mpsc::unbounded::<u64>(),
            proxy = true,
            label = "counter"
        );

        let start = Instant::now();
        for i in 0..runs {
            tx.unbounded_send(i).unwrap();
            spin_1us();
            rx.next().await.unwrap();
        }
        let elapsed = start.elapsed();

        println!(
            "futures channel: {runs} send/recv cycles in {elapsed:?} ({:.1} ns/op)",
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
