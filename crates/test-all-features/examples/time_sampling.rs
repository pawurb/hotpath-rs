//! Run with:
//!   cargo run -p test-all-features --example time_sampling --features hotpath
//!
//! Single-threaded workload for time-sampling integration tests.
//!
//! Sampling decisions are random, so the tests check that sampled counts land
//! near `rate * count` rather than on an exact value. `work_a` and `work_b`
//! alternate on purpose: a deterministic 1-in-2 sampler would time every call
//! of one and none of the other.

use std::time::Duration;

/// Events per resource; large enough for the sampled share to be checked
/// statistically.
const CALLS: u64 = 1000;

#[hotpath::measure]
fn work_a(i: u64) {
    std::hint::black_box(i);
    std::thread::sleep(Duration::from_micros(20));
}

#[hotpath::measure]
fn work_b(i: u64) {
    std::hint::black_box(i);
    std::thread::sleep(Duration::from_micros(20));
}

fn parse_rate(name: &str) -> Option<f64> {
    std::env::var(name).ok()?.parse().ok()
}

fn main() {
    let mut builder = hotpath::HotpathGuardBuilder::new("main").sections(vec![
        hotpath::Section::FunctionsTiming,
        hotpath::Section::FunctionsAlloc,
        hotpath::Section::Mutexes,
        hotpath::Section::RwLocks,
        hotpath::Section::Channels,
    ]);
    if let Some(rate) = parse_rate("TEST_BUILDER_TIME_SAMPLING_RATE") {
        builder = builder.time_sampling_rate(rate);
    }
    if let Some(rate) = parse_rate("TEST_BUILDER_FUNCTIONS_TIME_SAMPLING_RATE") {
        builder = builder.functions_time_sampling_rate(rate);
    }
    let _guard = builder.build();

    for i in 0..CALLS {
        work_a(i);
        work_b(i);
    }

    let mutex = hotpath::mutex!(std::sync::Mutex::new(0u64), label = "sampled_mutex");
    for _ in 0..CALLS {
        *mutex.lock().unwrap() += 1;
    }

    let rw = hotpath::rw_lock!(std::sync::RwLock::new(0u64), label = "sampled_rw");
    for _ in 0..CALLS {
        let _ = *rw.read().unwrap();
    }
    for _ in 0..CALLS / 2 {
        *rw.write().unwrap() += 1;
    }

    let (tx, rx) = hotpath::channel!(std::sync::mpsc::channel::<u64>(), label = "sampled_channel");
    for i in 0..CALLS {
        tx.send(i).unwrap();
    }
    for _ in 0..CALLS {
        let _ = rx.recv().unwrap();
    }
    drop(tx);
    drop(rx);

    println!("Time sampling example completed!");
}
