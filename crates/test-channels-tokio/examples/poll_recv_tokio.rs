// Demonstrates poll_recv/poll_recv_many on wrapped tokio channels, driven manually
// via poll_fn including a Pending-then-Ready sequence that exercises the reusable
// internal scratch buffer.
//
// cargo run -p test-channels-tokio --example poll_recv_tokio --features hotpath
use std::future::poll_fn;
use std::task::Poll;

#[tokio::main]
async fn main() {
    let guard = hotpath::HotpathGuardBuilder::new("main")
        .format(hotpath::Format::JsonPretty)
        .sections(vec![hotpath::Section::Channels])
        .build();

    let (tx, mut rx) = hotpath::channel!(
        tokio::sync::mpsc::channel::<i32>(50),
        label = "wrap-poll-recv"
    );

    let mut buf = Vec::new();
    let was_pending =
        poll_fn(|cx| Poll::Ready(rx.poll_recv_many(cx, &mut buf, 10).is_pending())).await;
    assert!(was_pending, "empty channel must poll Pending");
    assert!(buf.is_empty());

    for i in 0..30 {
        tx.send(i).await.expect("Failed to send");
    }

    let first = poll_fn(|cx| rx.poll_recv(cx)).await;
    assert_eq!(first, Some(0));

    while buf.len() < 29 {
        poll_fn(|cx| rx.poll_recv_many(cx, &mut buf, 10)).await;
    }
    assert_eq!(buf, (1..30).collect::<Vec<_>>());

    drop(tx);
    let closed = poll_fn(|cx| rx.poll_recv(cx)).await;
    assert_eq!(closed, None);

    drop(guard);

    if let Ok(secs) = std::env::var("TEST_SLEEP_SECONDS") {
        if let Ok(duration) = secs.parse::<u64>() {
            tokio::time::sleep(std::time::Duration::from_secs(duration)).await;
        }
    }

    println!("\nExample completed!");
}
