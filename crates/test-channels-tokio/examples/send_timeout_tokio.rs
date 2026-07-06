// Demonstrates send_timeout on a full wrapped bounded channel: the timed-out send
// rolls the depth counter back, so the report never shows a queue deeper than the
// channel capacity and the failed send is not counted.
//
// cargo run -p test-channels-tokio --example send_timeout_tokio --features hotpath
use std::time::Duration;

use tokio::sync::mpsc::error::SendTimeoutError;

#[tokio::main]
async fn main() {
    let guard = hotpath::HotpathGuardBuilder::new("main")
        .format(hotpath::Format::JsonPretty)
        .sections(vec![hotpath::Section::Channels])
        .build();

    let (tx, mut rx) = hotpath::channel!(
        tokio::sync::mpsc::channel::<i32>(5),
        label = "wrap-send-timeout"
    );

    for i in 0..5 {
        tx.send(i).await.expect("Failed to send");
    }

    let err = tx.send_timeout(99, Duration::from_millis(50)).await;
    assert!(
        matches!(err, Err(SendTimeoutError::Timeout(99))),
        "expected timeout on full channel"
    );

    // Generate the report while the channel is still full.
    drop(guard);

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
