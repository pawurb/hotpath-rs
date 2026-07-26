//! Run with:
//!   cargo run -p test-channels-tokio --example recv_many_tokio --features hotpath
// Demonstrates batch receiving via recv_many on wrapped tokio channels: every
// message gets its own receive event, so counts, delay histograms, and queue depth
// stay exact even when messages are drained in batches.
#[tokio::main]
async fn main() {
    let guard = hotpath::HotpathGuardBuilder::new("main")
        .format(hotpath::Format::JsonPretty)
        .sections(vec![hotpath::Section::Channels])
        .build();

    let (tx, mut rx) = hotpath::channel!(
        tokio::sync::mpsc::channel::<i32>(100),
        label = "wrap-recv-many"
    );

    for i in 0..60 {
        tx.send(i).await.expect("Failed to send");
    }

    let mut buf = Vec::new();
    assert_eq!(rx.recv_many(&mut buf, 40).await, 40);
    assert_eq!(rx.recv_many(&mut buf, 40).await, 20);
    assert_eq!(buf, (0..60).collect::<Vec<_>>());
    assert!(rx.is_empty());

    let (utx, mut urx) = hotpath::channel!(
        tokio::sync::mpsc::unbounded_channel::<i32>(),
        label = "wrap-recv-many-unbounded"
    );
    for i in 0..30 {
        utx.send(i).expect("Failed to send");
    }
    let mut ubuf = Vec::new();
    assert_eq!(urx.recv_many(&mut ubuf, 100).await, 30);
    assert_eq!(ubuf, (0..30).collect::<Vec<_>>());

    drop(guard);

    if let Ok(secs) = std::env::var("TEST_SLEEP_SECONDS") {
        if let Ok(duration) = secs.parse::<u64>() {
            tokio::time::sleep(std::time::Duration::from_secs(duration)).await;
        }
    }

    println!("\nExample completed!");
}
