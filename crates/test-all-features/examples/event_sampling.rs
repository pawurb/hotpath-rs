//! Run with:
//!   cargo run -p test-all-features --example event_sampling --features hotpath
//!
//! Deterministic single-threaded workload for event-sampling integration
//! tests. Everything runs on the main thread (sync calls plus a
//! current-thread tokio runtime), so the thread-local event counter is
//! deterministic: the counter starts at 0 and 0 is always kept, so rate 0.5
//! over 10 calls keeps exactly 5.
//!
//! `work` allocates exactly 1 KB per call so scaled allocation totals are
//! deterministic under `hotpath-alloc`.

use std::time::Duration;

#[hotpath::measure]
fn work(i: u64) {
    let buf = vec![i as u8; 1024];
    std::hint::black_box(buf);
    std::thread::sleep(Duration::from_micros(50));
}

#[hotpath::measure]
async fn async_work(i: u64) {
    std::hint::black_box(i);
    tokio::time::sleep(Duration::from_micros(50)).await;
}

fn parse_rate(name: &str) -> Option<f64> {
    std::env::var(name).ok()?.parse().ok()
}

fn main() {
    let mut builder = hotpath::HotpathGuardBuilder::new("main").sections(vec![
        hotpath::Section::FunctionsTiming,
        hotpath::Section::FunctionsAlloc,
    ]);
    if let Some(rate) = parse_rate("TEST_BUILDER_FUNCTIONS_EVENT_SAMPLING_RATE") {
        builder = builder.functions_event_sampling_rate(rate);
    }
    let _guard = builder.build();

    for i in 0..10 {
        work(i);
    }

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .unwrap();
    runtime.block_on(async {
        for i in 0..10 {
            async_work(i).await;
        }
    });

    println!("Event sampling example completed!");
}
