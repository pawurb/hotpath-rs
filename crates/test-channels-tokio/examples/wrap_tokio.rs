//! Run with:
//!   cargo run -p test-channels-tokio --example wrap_tokio --features hotpath
// Demonstrates tokio::sync::mpsc channel instrumentation: the report shows the
// exact queue depth (50 messages parked in the channel) because the instrumented
// endpoints track queue length with a self-maintained counter instead of routing
// through a forwarder task and a second channel.
//
// Tokio bounded channels recover their capacity from `Sender::max_capacity()`, so no
// `capacity = N` argument is needed.
#[tokio::main]
async fn main() {
    let guard = hotpath::HotpathGuardBuilder::new("main")
        .format(hotpath::Format::JsonPretty)
        .sections(vec![hotpath::Section::Channels])
        .build();

    // Returns hotpath::wrap::tokio::sync::mpsc::{Sender, Receiver}.
    let (tx, mut rx) =
        hotpath::channel!(tokio::sync::mpsc::channel::<i32>(100), label = "wrap-queue");

    // Park 50 messages in the channel without receiving any.
    for i in 0..50 {
        tx.send(i).await.expect("Failed to send");
    }

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
            tokio::time::sleep(std::time::Duration::from_secs(duration)).await;
        }
    }

    println!("\nExample completed!");
}
