//! Run with:
//!   cargo run -p test-channels-tokio --example agg_queue_tokio --features hotpath

// Two live channel instances from one call site each hold unreceived messages
// at guard drop: the aggregated entry must report the combined in-flight depth
// (6), not the largest single-instance snapshot (3).

#[tokio::main]
async fn main() {
    let _channels_guard = hotpath::HotpathGuardBuilder::new("main")
        .format(hotpath::Format::JsonPretty)
        .sections(vec![hotpath::Section::Channels])
        .build();

    let mut kept = Vec::new();
    for i in 0..2i32 {
        let (tx, rx) = hotpath::channel!(tokio::sync::mpsc::channel::<i32>(10));
        for _ in 0..3 {
            tx.send(i).await.expect("Failed to send");
        }
        kept.push((tx, rx));
    }

    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

    drop(_channels_guard);
    drop(kept);

    println!("Aggregated queue example completed!");
}
