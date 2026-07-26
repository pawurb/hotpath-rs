//! Run with:
//!   cargo run -p test-channels-asc --example wrap_asc --features hotpath
// Demonstrates async-channel instrumentation: the report shows the exact
// queue depth (50 messages parked in the channel) because the instrumented endpoints
// sample the real channel length instead of routing through a forwarder task.
use std::thread;
use std::time::Duration;

fn main() {
    smol::block_on(async {
        let guard = hotpath::HotpathGuardBuilder::new("main")
            .format(hotpath::Format::JsonPretty)
            .sections(vec![hotpath::Section::Channels])
            .build();

        // Returns hotpath::wrap::async_channel::{Sender, Receiver}.
        let (tx, rx) = hotpath::channel!(async_channel::bounded::<i32>(100), label = "wrap-queue");

        // Park 50 messages in the channel without receiving any.
        for i in 0..50 {
            tx.send(i).await.expect("Failed to send");
        }

        println!("[main] queued (live len) = {}", tx.len());

        // Generate the report while 50 messages are still in flight.
        drop(guard);

        // Drain afterwards so the receiver is exercised too.
        let mut drained = 0;
        while rx.try_recv().is_ok() {
            drained += 1;
        }
        println!("[main] drained {} messages", drained);

        if let Ok(secs) = std::env::var("TEST_SLEEP_SECONDS") {
            if let Ok(duration) = secs.parse::<u64>() {
                thread::sleep(Duration::from_secs(duration));
            }
        }

        println!("\nExample completed!");
    })
}
