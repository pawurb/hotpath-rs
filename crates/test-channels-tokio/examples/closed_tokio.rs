#[allow(unused_mut)]
#[tokio::main]
async fn main() {
    #[cfg(feature = "hotpath")]
    let _channels_guard = hotpath::channels::ChannelsGuardBuilder::new().build();

    let (txa, mut rxa) = tokio::sync::mpsc::unbounded_channel::<i32>();

    #[cfg(feature = "hotpath")]
    let (txa, mut rxa) = hotpath::channel!((txa, rxa), label = "unbounded-channel");

    let (txb, mut rxb) = tokio::sync::mpsc::channel::<i32>(10);

    #[cfg(feature = "hotpath")]
    let (txb, mut rxb) = hotpath::channel!((txb, rxb), label = "bounded-channel");

    let (txc, rxc) = tokio::sync::oneshot::channel::<String>();

    #[cfg(feature = "hotpath")]
    let (txc, rxc) = hotpath::channel!((txc, rxc), label = "oneshot-channel");

    println!("[Unbounded] Sending 3 messages...");
    for i in 1..=3 {
        txa.send(i).expect("Failed to send");
    }

    for _ in 0..3 {
        if let Some(msg) = rxa.recv().await {
            println!("[Unbounded] Received: {}", msg);
        }
    }

    println!("[Bounded] Sending 3 messages...");
    for i in 1..=3 {
        txb.send(i).await.expect("Failed to send");
    }

    for _ in 0..3 {
        if let Some(msg) = rxb.recv().await {
            println!("[Bounded] Received: {}", msg);
        }
    }

    println!("[Oneshot] Sending message...");
    txc.send("Hello from oneshot!".to_string())
        .expect("Failed to send oneshot");

    match rxc.await {
        Ok(msg) => println!("[Oneshot] Received: {}", msg),
        Err(_) => println!("[Oneshot] Sender dropped"),
    }

    println!("\nClosing channels from receiver side...");

    drop(rxa);
    println!("[Unbounded] Receiver closed");

    drop(rxb);
    println!("[Bounded] Receiver closed");

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    println!("\nAttempting to send after closing receivers...");

    match txa.send(999) {
        Ok(_) => panic!("[Unbounded] Send succeeded (unexpected)"),
        Err(_) => println!("[Unbounded] Send failed - channel closed"),
    }

    match txb.send(999).await {
        Ok(_) => panic!("[Bounded] Send succeeded (unexpected)"),
        Err(_) => println!("[Bounded] Send failed - channel closed"),
    }

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    println!("\nExample completed!");
}
