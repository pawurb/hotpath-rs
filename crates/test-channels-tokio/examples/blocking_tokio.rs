//! Run with:
//!   cargo run -p test-channels-tokio --example blocking_tokio --features hotpath
// Demonstrates blocking_send/blocking_recv/blocking_recv_many on wrapped tokio
// channels, driven from plain std threads with no async runtime: event emission is
// a sync crossbeam send, so stats are recorded off-runtime without panics.
fn main() {
    let guard = hotpath::HotpathGuardBuilder::new("main")
        .format(hotpath::Format::JsonPretty)
        .sections(vec![hotpath::Section::Channels])
        .build();

    let (tx, mut rx) = hotpath::channel!(
        tokio::sync::mpsc::channel::<i32>(10),
        label = "wrap-blocking"
    );

    let producer = std::thread::spawn(move || {
        for i in 0..25 {
            tx.blocking_send(i).expect("Failed to send");
        }
    });

    let consumer = std::thread::spawn(move || {
        let mut buf = Vec::new();
        while let Some(v) = rx.blocking_recv() {
            buf.push(v);
            if rx.blocking_recv_many(&mut buf, 8) == 0 {
                break;
            }
        }
        buf
    });

    producer.join().expect("producer panicked");
    let buf = consumer.join().expect("consumer panicked");
    assert_eq!(buf, (0..25).collect::<Vec<_>>());

    drop(guard);

    if let Ok(secs) = std::env::var("TEST_SLEEP_SECONDS") {
        if let Ok(duration) = secs.parse::<u64>() {
            std::thread::sleep(std::time::Duration::from_secs(duration));
        }
    }

    println!("\nExample completed!");
}
