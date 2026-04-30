//! CPU profiling with worker threads.
//!
//! Spawns N CPU-bound worker threads while `main` mostly sleeps. Useful to
//! validate caller_name attribution: under the current implementation,
//! caller_name (`main`) is reported as 100% of total_samples, but worker
//! threads contribute samples too — so the wrapper percentage is misleading
//! when work happens off the main thread.
//!
//! Profile with hotpath:
//! ```bash
//! cargo run --example cpu_multithread --features hotpath --profile profiling
//! ```

use std::hint::black_box;
use std::thread;
use std::time::Duration;

#[hotpath::measure]
fn worker_loop(iterations: u32) -> u64 {
    let mut result: u64 = 1;
    for i in 0..iterations {
        result = result.wrapping_mul(black_box(i as u64).wrapping_add(7));
        result ^= result >> 3;
    }
    result
}

#[hotpath::main]
fn main() {
    let handles: Vec<_> = (0..4)
        .map(|_| thread::spawn(|| black_box(worker_loop(20_000_000))))
        .collect();

    thread::sleep(Duration::from_secs(2));

    for h in handles {
        let _ = h.join();
    }
}
