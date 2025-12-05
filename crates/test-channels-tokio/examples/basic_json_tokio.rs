#[allow(unused_mut)]
#[tokio::main]
async fn main() {
    let _channels_guard = hotpath::channels::ChannelsGuardBuilder::new()
        .format(hotpath::Format::JsonPretty)
        .build();

    let (txa, _rxa) = hotpath::channel!(tokio::sync::mpsc::unbounded_channel::<i32>());

    let (txb, mut rxb) = hotpath::channel!(tokio::sync::mpsc::channel::<i32>(10));

    let (txc, rxc) = hotpath::channel!(
        tokio::sync::oneshot::channel::<String>(),
        label = "hello-there"
    );

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

    while let Some(msg) = rxb.recv().await {
        println!("[Receiver] Received message: {}", msg);
    }

    println!("\nExample completed!");
}
