// Dropping the (single) receiver while a sender is still alive disconnects the channel.
// The endpoint wrapper must report the channel as `closed`. std::sync::mpsc receivers
// are not Clone, so this is the only consumer.
//
// cargo run -p test-channels-std --example wrap_closed_std --features hotpath
use std::sync::mpsc;

fn main() {
    let guard = hotpath::HotpathGuardBuilder::new("main")
        .format(hotpath::Format::JsonPretty)
        .sections(vec![hotpath::Section::Channels])
        .build();

    let (tx, rx) = hotpath::channel!(mpsc::channel::<i32>(), wrap = true, label = "recv-dropped");

    tx.send(1).expect("Failed to send");

    drop(rx);

    assert!(
        tx.send(2).is_err(),
        "send should fail after receiver dropped"
    );

    drop(guard);

    println!("\nExample completed!");
}
