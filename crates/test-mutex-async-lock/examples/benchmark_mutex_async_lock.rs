use futures_lite::future;
use std::time::Instant;

// Simple single-threaded stress test: hammers a single instrumented mutex in a
// tight loop with no contention, so the measured time reflects per-lock
// instrumentation overhead. Compare `--features hotpath` against a plain run.
fn main() {
    let _guard = hotpath::HotpathGuardBuilder::new("main")
        .sections(vec![hotpath::Section::Mutexes])
        .build();

    let runs = bench_runs();

    future::block_on(async {
        let lock = hotpath::mutex!(async_lock::Mutex::new(0u64), label = "counter");

        let start = Instant::now();
        for _ in 0..runs {
            let mut v = lock.lock().await;
            *v += 1;
        }
        let elapsed = start.elapsed();

        println!(
            "async-lock Mutex: {runs} lock cycles in {elapsed:?} ({:.1} ns/op)",
            elapsed.as_nanos() as f64 / runs as f64
        );
        println!("Final value: {}", *lock.lock().await);
    });
}

fn bench_runs() -> u64 {
    std::env::var("HOTPATH_LOCK_BENCH_RUNS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1_000_000)
}
