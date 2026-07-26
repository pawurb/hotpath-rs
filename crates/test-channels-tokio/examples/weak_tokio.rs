//! Run with:
//!   cargo run -p test-channels-tokio --example weak_tokio --features hotpath
// Demonstrates WeakSender/WeakUnboundedSender on wrapped tokio channels:
// downgrade/upgrade lifecycle, strong/weak counts matching the receiver-side view,
// and the channel closing once all strong senders are gone.
#[tokio::main]
async fn main() {
    let guard = hotpath::HotpathGuardBuilder::new("main")
        .format(hotpath::Format::JsonPretty)
        .sections(vec![hotpath::Section::Channels])
        .build();

    let (tx, mut rx) = hotpath::channel!(tokio::sync::mpsc::channel::<i32>(8), label = "wrap-weak");

    let weak = tx.downgrade();
    assert_eq!(tx.strong_count(), 1);
    assert_eq!(tx.weak_count(), 1);

    let tx2 = weak.upgrade().expect("upgrade while strong sender alive");
    assert_eq!(rx.sender_strong_count(), tx.strong_count());
    assert_eq!(rx.sender_weak_count(), tx.weak_count());
    assert!(tx.same_channel(&tx2));

    tx.send(1).await.expect("Failed to send");
    tx2.send(2).await.expect("Failed to send");
    assert_eq!(rx.recv().await, Some(1));
    assert_eq!(rx.recv().await, Some(2));

    drop(tx);
    drop(tx2);
    assert!(
        weak.upgrade().is_none(),
        "upgrade must fail after all strong senders dropped"
    );

    let (utx, mut urx) = hotpath::channel!(
        tokio::sync::mpsc::unbounded_channel::<i32>(),
        label = "wrap-weak-unbounded"
    );
    let uweak = utx.downgrade();
    let utx2 = uweak.upgrade().expect("upgrade while strong sender alive");
    assert_eq!(urx.sender_strong_count(), 2);
    assert_eq!(urx.sender_weak_count(), 1);
    utx.send(1).expect("Failed to send");
    utx2.send(2).expect("Failed to send");
    assert_eq!(urx.recv().await, Some(1));
    assert_eq!(urx.recv().await, Some(2));
    drop(utx);
    drop(utx2);
    assert!(uweak.upgrade().is_none());

    drop(guard);

    if let Ok(secs) = std::env::var("TEST_SLEEP_SECONDS") {
        if let Ok(duration) = secs.parse::<u64>() {
            tokio::time::sleep(std::time::Duration::from_secs(duration)).await;
        }
    }

    println!("\nExample completed!");
}
