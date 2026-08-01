//! Run with:
//!   cargo run -p test-channels-tokio --example agg_tokio --features hotpath

// Creates several channels at one call site in the default (aggregated) mode:
// the JSON report at guard drop must contain a single entry for the call site
// with `instances == 5`, summed counts, and a closed derived state.

#[tokio::main]
async fn main() {
    let _channels_guard = hotpath::HotpathGuardBuilder::new("main")
        .format(hotpath::Format::JsonPretty)
        .sections(vec![hotpath::Section::Channels])
        .build();

    for i in 0..5i32 {
        let (tx, mut rx) = hotpath::channel!(tokio::sync::mpsc::channel::<i32>(10));
        tx.send(i).await.expect("Failed to send");
        tx.send(i).await.expect("Failed to send");
        let _ = rx.recv().await;
        let _ = rx.recv().await;
    }

    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

    drop(_channels_guard);

    println!("Aggregation example completed!");
}
