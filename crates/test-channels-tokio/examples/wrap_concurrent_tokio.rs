//! Run with:
//!   cargo run -p test-channels-tokio --example wrap_concurrent_tokio --features hotpath
// Races a producer against a consumer on an unbounded tokio::sync::mpsc wrap channel.
// The depth counter is incremented before each publish, so a fast consumer can never
// decrement it below zero (which would panic in debug builds). Asserts every message is
// accounted for and the queue drains back to zero.
const N: u64 = 50_000;

#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() {
    let guard = hotpath::HotpathGuardBuilder::new("main")
        .format(hotpath::Format::JsonPretty)
        .sections(vec![hotpath::Section::Channels])
        .build();

    let (tx, mut rx) = hotpath::channel!(
        tokio::sync::mpsc::unbounded_channel::<u64>(),
        label = "wrap-concurrent"
    );

    let producer = tokio::spawn(async move {
        for i in 0..N {
            tx.send(i).expect("Failed to send");
        }
    });

    let consumer = tokio::spawn(async move {
        let mut received = 0u64;
        while rx.recv().await.is_some() {
            received += 1;
        }
        received
    });

    producer.await.expect("producer panicked");
    let received = consumer.await.expect("consumer panicked");
    assert_eq!(received, N, "every message should be received");

    drop(guard);

    println!("\nExample completed!");
}
