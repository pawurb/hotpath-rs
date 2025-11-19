#[tokio::main]
async fn main() {
    #[cfg(feature = "hotpath")]
    let _channels_guard = hotpath::channels::ChannelsGuard::new();

    let (tx, rx) = tokio::sync::oneshot::channel::<String>();

    #[cfg(feature = "hotpath")]
    let (tx, rx) = hotpath::channel!((tx, rx));

    drop(rx);

    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    match tx.send("Hello oneshot!".to_string()) {
        Ok(_) => panic!("Not expected: send succeeded"),
        Err(_) => println!("Expected: Failed to send"),
    }
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    println!("\nExample completed!");
}
