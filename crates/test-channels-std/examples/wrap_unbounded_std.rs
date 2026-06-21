// Demonstrates `wrap = true` std::sync::mpsc instrumentation on an unbounded channel.
// Sends N messages, drains them all, and the report reflects exact sent/received
// counts with the self-tracked queue draining back to zero.
//
// Unbounded std wrap needs no `capacity`.
//
// cargo run -p test-channels-std --example wrap_unbounded_std --features hotpath
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

fn main() {
    let guard = hotpath::HotpathGuardBuilder::new("main")
        .format(hotpath::Format::JsonPretty)
        .sections(vec![hotpath::Section::Channels])
        .build();

    // wrap = true returns hotpath::wrap::std::sync::mpsc::{Sender, Receiver}.
    let (tx, rx) = hotpath::channel!(
        mpsc::channel::<i32>(),
        wrap = true,
        label = "wrap-unbounded"
    );

    for i in 0..200 {
        tx.send(i).expect("Failed to send");
    }

    let drained: Vec<i32> = rx.try_iter().collect();
    println!("[main] drained {} messages", drained.len());

    drop(guard);

    if let Ok(secs) = std::env::var("TEST_SLEEP_SECONDS") {
        if let Ok(duration) = secs.parse::<u64>() {
            thread::sleep(Duration::from_secs(duration));
        }
    }

    println!("\nExample completed!");
}
