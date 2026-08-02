//! Run with:
//!   cargo run -p test-channels-tokio --example agg_many_tokio --features hotpath

// Boundedness smoke: creates a few thousand default-mode channels at one call
// site; profiler state must stay at a single entry aggregating them all.

#[tokio::main]
async fn main() {
    let _channels_guard = hotpath::HotpathGuardBuilder::new("main")
        .format(hotpath::Format::JsonPretty)
        .sections(vec![hotpath::Section::Channels])
        .build();

    for i in 0..2000i32 {
        let (tx, mut rx) = hotpath::channel!(tokio::sync::mpsc::channel::<i32>(4));
        tx.send(i).await.expect("Failed to send");
        let _ = rx.recv().await;
    }

    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

    drop(_channels_guard);

    println!("Boundedness example completed!");
}
