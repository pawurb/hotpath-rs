// Demonstrates `wrap = true` std::sync::mpsc instrumentation: the report shows the
// exact queue depth (50 messages parked in the channel) because the instrumented
// endpoints track queue length with a self-maintained counter instead of routing
// through a forwarder thread.
//
// Bounded std channels (`sync_channel`) cannot recover their capacity from the
// endpoint, so `capacity = N` must be passed to the macro - and it must match the
// `sync_channel(N)` argument, since wrap mode rebuilds the channel from `capacity`.
//
// cargo run -p test-channels-std --example wrap_std --features hotpath
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

fn main() {
    let guard = hotpath::HotpathGuardBuilder::new("main")
        .format(hotpath::Format::JsonPretty)
        .sections(vec![hotpath::Section::Channels])
        .build();

    // wrap = true returns hotpath::wrap::std::sync::mpsc::{SyncSender, Receiver}.
    let (tx, rx) = hotpath::channel!(
        mpsc::sync_channel::<i32>(100),
        wrap = true,
        capacity = 100,
        label = "wrap-queue"
    );

    // Park 50 messages in the channel without receiving any.
    for i in 0..50 {
        tx.send(i).expect("Failed to send");
    }

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
