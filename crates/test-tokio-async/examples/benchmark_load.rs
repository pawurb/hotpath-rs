//! Run with:
//!   cargo run -p test-tokio-async --example benchmark_load --features hotpath

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

#[hotpath::measure]
fn noop() {}

fn run_load(start: Instant, duration: Duration, rate: u64) -> u64 {
    let mut count: u64 = 0;

    if rate == 0 {
        while start.elapsed() < duration {
            noop();
            count += 1;
        }
    } else {
        // `rate` is the per-thread target.
        let interval_secs = 1.0 / rate as f64;
        loop {
            if start.elapsed() >= duration {
                break;
            }
            // Busy-spin to the next deadline for precise pacing.
            let deadline = Duration::from_secs_f64(count as f64 * interval_secs);
            while start.elapsed() < deadline {
                std::hint::spin_loop();
            }
            noop();
            count += 1;
        }
    }

    count
}

#[hotpath::main]
fn main() {
    let rate: u64 = std::env::var("HOTPATH_BENCH_MSG_PER_S")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);

    let threads: usize = std::env::var("HOTPATH_BENCH_THREADS")
        .ok()
        .and_then(|v| v.parse().ok())
        .filter(|&t| t > 0)
        .unwrap_or(1);

    let duration_secs: f64 = std::env::var("HOTPATH_BENCH_DURATION_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(5.0);
    let duration = Duration::from_secs_f64(duration_secs);

    let start = Instant::now();
    let total = Arc::new(AtomicU64::new(0));

    let handles: Vec<_> = (1..threads)
        .map(|_| {
            let total = Arc::clone(&total);
            std::thread::spawn(move || {
                let n = run_load(start, duration, rate);
                total.fetch_add(n, Ordering::Relaxed);
            })
        })
        .collect();

    // Drive one producer on the main thread too.
    total.fetch_add(run_load(start, duration, rate), Ordering::Relaxed);

    for h in handles {
        h.join().unwrap();
    }

    let elapsed = start.elapsed();
    let count = total.load(Ordering::Relaxed);
    let achieved = count as f64 / elapsed.as_secs_f64();

    let target = if rate == 0 {
        "peak".to_string()
    } else {
        format!("{rate}/s/thread")
    };

    println!(
        "load: {count} calls across {threads} thread(s) in {elapsed:?} \
         (target: {target}, achieved: {achieved:.0} msg/s, {:.1} ns/op)",
        elapsed.as_nanos() as f64 / count.max(1) as f64
    );
}
