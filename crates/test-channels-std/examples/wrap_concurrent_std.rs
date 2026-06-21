// Races a producer against a consumer on an unbounded std::sync::mpsc wrap channel.
// The depth counter is incremented before each publish, so a fast consumer can never
// decrement it below zero (which would panic in debug builds). Asserts every message
// is accounted for and the queue drains back to zero.
//
// cargo run -p test-channels-std --example wrap_concurrent_std --features hotpath
use std::sync::mpsc;
use std::thread;

const N: u64 = 50_000;

fn main() {
    let guard = hotpath::HotpathGuardBuilder::new("main")
        .format(hotpath::Format::JsonPretty)
        .sections(vec![hotpath::Section::Channels])
        .build();

    let (tx, rx) = hotpath::channel!(
        mpsc::channel::<u64>(),
        wrap = true,
        label = "wrap-concurrent"
    );

    let producer = thread::spawn(move || {
        for i in 0..N {
            tx.send(i).expect("Failed to send");
        }
    });

    let consumer = thread::spawn(move || {
        let mut received = 0u64;
        while rx.recv().is_ok() {
            received += 1;
        }
        received
    });

    producer.join().expect("producer panicked");
    let received = consumer.join().expect("consumer panicked");
    assert_eq!(received, N, "every message should be received");

    drop(guard);

    println!("\nExample completed!");
}
