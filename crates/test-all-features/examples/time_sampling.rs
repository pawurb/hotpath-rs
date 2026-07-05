//! Deterministic single-threaded workload for time-sampling integration tests.
//!
//! Counts are chosen so 1-in-k sampling yields exact sampled counts: the
//! thread-local counters and the per-channel `msg_id` both start at 0 and 0 is
//! always sampled, so rate 0.5 over 10 events samples exactly 5.

use std::time::Duration;

#[hotpath::measure]
fn work(i: u64) {
    std::hint::black_box(i);
    std::thread::sleep(Duration::from_micros(50));
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

    for i in 0..10 {
        work(i);
    }

    let mutex = hotpath::mutex!(std::sync::Mutex::new(0u64), label = "sampled_mutex");
    for _ in 0..10 {
        *mutex.lock().unwrap() += 1;
    }

    let rw = hotpath::rw_lock!(std::sync::RwLock::new(0u64), label = "sampled_rw");
    for _ in 0..10 {
        let _ = *rw.read().unwrap();
    }
    for _ in 0..4 {
        *rw.write().unwrap() += 1;
    }

    let (tx, rx) = hotpath::channel!(std::sync::mpsc::channel::<u64>(), label = "sampled_channel");
    for i in 0..10u64 {
        tx.send(i).unwrap();
    }
    for _ in 0..10 {
        let _ = rx.recv().unwrap();
    }
    drop(tx);
    drop(rx);

    println!("Time sampling example completed!");
}
