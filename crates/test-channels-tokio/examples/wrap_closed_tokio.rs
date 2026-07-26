//! Run with:
//!   cargo run -p test-channels-tokio --example wrap_closed_tokio --features hotpath
// Dropping the (single) receiver while a sender is still alive disconnects the channel.
// The endpoint wrapper must report the channel as `closed`. tokio::sync::mpsc receivers
// are not Clone, so this is the only consumer.
#[tokio::main]
async fn main() {
    let guard = hotpath::HotpathGuardBuilder::new("main")
        .format(hotpath::Format::JsonPretty)
        .sections(vec![hotpath::Section::Channels])
        .build();

    let (tx, rx) = hotpath::channel!(
        tokio::sync::mpsc::unbounded_channel::<i32>(),
        label = "recv-dropped"
    );

    tx.send(1).expect("Failed to send");

    drop(rx);

    assert!(
        tx.send(2).is_err(),
        "send should fail after receiver dropped"
    );

    drop(guard);

    println!("\nExample completed!");
}
