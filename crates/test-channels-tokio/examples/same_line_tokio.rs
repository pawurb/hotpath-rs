//! Run with:
//!   cargo run -p test-channels-tokio --example same_line_tokio --features hotpath

// Two `channel!` invocations on one physical line with the same message type:
// the registration key includes the column, so each call site keeps its own
// entry (labels, kind/capacity, counts) instead of aliasing into the first
// invocation's id.

#[tokio::main]
async fn main() {
    let _channels_guard = hotpath::HotpathGuardBuilder::new("main")
        .format(hotpath::Format::JsonPretty)
        .sections(vec![hotpath::Section::Channels])
        .build();

    #[rustfmt::skip]
    let ((tx_a, _rx_a), (tx_b, _rx_b)) = (hotpath::channel!(tokio::sync::mpsc::channel::<i32>(4), label = "same-line-a"), hotpath::channel!(tokio::sync::mpsc::channel::<i32>(8), label = "same-line-b"));

    for i in 0..3 {
        tx_a.send(i).await.expect("Failed to send");
    }
    for i in 0..5 {
        tx_b.send(i).await.expect("Failed to send");
    }

    drop(_channels_guard);

    println!("Same-line channels example completed!");
}
