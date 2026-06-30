// Demonstrates `wrap = true` flume instrumentation: the report shows the exact
// queue depth (50 messages parked in the channel) because the instrumented endpoints
// sample the real channel length instead of routing through a forwarder task.
//
// cargo run -p test-channels-flume --example wrap_flume --features hotpath
use std::thread;
use std::time::Duration;

fn main() {
    let guard = hotpath::HotpathGuardBuilder::new("main")
        .format(hotpath::Format::JsonPretty)
        .sections(vec![hotpath::Section::Channels])
        .build();

    // wrap = true returns hotpath::wrap::flume::{Sender, Receiver}.
    let (tx, rx) = hotpath::channel!(
        flume::bounded::<i32>(100),
        wrap = true,
        label = "wrap-queue"
    );

    // Park 50 messages in the channel without receiving any.
    for i in 0..50 {
        tx.send(i).expect("Failed to send");
    }

    println!("[main] queued (live len) = {}", tx.len());

    // Generate the report while 50 messages are still in flight.
    drop(guard);

    // Drain afterwards so the receiver is exercised too.
    let drained: Vec<i32> = rx.try_iter().collect();
    println!("[main] drained {} messages", drained.len());

    if let Ok(secs) = std::env::var("TEST_SLEEP_SECONDS") {
        if let Ok(duration) = secs.parse::<u64>() {
            thread::sleep(Duration::from_secs(duration));
        }
    }

    println!("\nExample completed!");
}
