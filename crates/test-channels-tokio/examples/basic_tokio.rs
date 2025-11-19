#[allow(dead_code)]
struct Actor {
    name: String,
}

#[allow(unused_mut)]
#[tokio::main]
async fn main() {
    let _actor1 = Actor {
        name: "Actor 1".to_string(),
    };

    #[cfg(feature = "hotpath")]
    let _channels_guard = hotpath::channels::ChannelsGuard::new();

    let (txa, mut _rxa) = tokio::sync::mpsc::unbounded_channel::<i32>();

    #[cfg(feature = "hotpath")]
    let (txa, _rxa) = hotpath::channel!((txa, _rxa), log = true, label = _actor1.name);

    let (txb, mut rxb) = tokio::sync::mpsc::channel::<i32>(10);
    #[cfg(feature = "hotpath")]
    let (txb, mut rxb) = hotpath::channel!((txb, rxb), label = "bounded-channel");

    let (txc, rxc) = tokio::sync::oneshot::channel::<String>();
    #[cfg(feature = "hotpath")]
    let (txc, rxc) = hotpath::channel!((txc, rxc), label = "hello-there");

    let sender_handle = tokio::spawn(async move {
        for i in 1..=3 {
            println!("[Sender] Sending message: {}", i);
            txa.send(i).expect("Failed to send");
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        }

        for i in 1..=3 {
            println!("[Sender] Sending message: {}", i);
            txb.send(i).await.expect("Failed to send");
            tokio::time::sleep(tokio::time::Duration::from_millis(250)).await;
        }

        println!("[Sender] Done sending messages");
    });

    let oneshot_receiver_handle = tokio::spawn(async move {
        match rxc.await {
            Ok(msg) => println!("[Oneshot] Received: {}", msg),
            Err(_) => println!("[Oneshot] Sender dropped"),
        }
    });

    println!("[Oneshot] Sending message");
    txc.send("Hello from oneshot!".to_string())
        .expect("Failed to send oneshot");

    sender_handle.await.expect("Sender task failed");
    oneshot_receiver_handle
        .await
        .expect("Oneshot receiver task failed");

    #[cfg(feature = "hotpath")]
    drop(_channels_guard);

    while let Some(msg) = rxb.recv().await {
        println!("[Receiver] Received message: {}", msg);
    }

    println!("\nExample completed!");
}
