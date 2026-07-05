// Demonstrates tokio::sync::mpsc channel instrumentation on an unbounded channel.
// Sends N messages, drains them all, and the report reflects exact sent/received counts
// with the self-tracked queue draining back to zero.
//
// cargo run -p test-channels-tokio --example wrap_unbounded_tokio --features hotpath
#[tokio::main]
async fn main() {
    let guard = hotpath::HotpathGuardBuilder::new("main")
        .format(hotpath::Format::JsonPretty)
        .sections(vec![hotpath::Section::Channels])
        .build();

    // Returns hotpath::wrap::tokio::sync::mpsc::{UnboundedSender, UnboundedReceiver}.
    let (tx, mut rx) = hotpath::channel!(
        tokio::sync::mpsc::unbounded_channel::<i32>(),
        label = "wrap-unbounded"
    );

    for i in 0..200 {
        tx.send(i).expect("Failed to send");
    }

    let mut drained = 0;
    while rx.try_recv().is_ok() {
        drained += 1;
    }
    println!("[main] drained {} messages", drained);

    drop(guard);

    if let Ok(secs) = std::env::var("TEST_SLEEP_SECONDS") {
        if let Ok(duration) = secs.parse::<u64>() {
            tokio::time::sleep(std::time::Duration::from_secs(duration)).await;
        }
    }

    println!("\nExample completed!");
}
