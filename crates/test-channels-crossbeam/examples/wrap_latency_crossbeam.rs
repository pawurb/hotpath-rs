//! Run with:
//!   cargo run -p test-channels-crossbeam --example wrap_latency_crossbeam --features hotpath
// Demonstrates the channel processing-time histogram: each message is held in the
// channel for a known delay before being received, so the report's `delay_avg` and
// `delay_percentiles` reflect the exact send->receive latency.
use std::thread;
use std::time::Duration;

fn main() {
    let guard = hotpath::HotpathGuardBuilder::new("main")
        .format(hotpath::Format::JsonPretty)
        .sections(vec![hotpath::Section::Channels])
        .percentiles(&[50.0, 95.0])
        .build();

    // Exact send->receive latency histogram.
    let (wtx, wrx) = hotpath::channel!(
        crossbeam_channel::unbounded::<i32>(),
        label = "wrap-latency"
    );

    for i in 0..10 {
        wtx.send(i).expect("Failed to send");
    }

    // Hold messages so the recorded send->receive latency is dominated by this sleep.
    thread::sleep(Duration::from_millis(20));

    let wrap_drained: Vec<i32> = wrx.try_iter().collect();
    println!("[main] drained {}", wrap_drained.len());

    drop(guard);

    println!("\nExample completed!");
}
