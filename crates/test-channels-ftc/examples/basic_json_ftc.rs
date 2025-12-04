use futures_util::stream::StreamExt;
use smol::Timer;
use std::time::Duration;

#[allow(unused_mut)]
fn main() {
    smol::block_on(async {
        let _channels_guard =
            hotpath::channels::ChannelsGuard::new().format(hotpath::Format::JsonPretty);

        let (txa, mut _rxa) = hotpath::channel!(
            futures_channel::mpsc::unbounded::<i32>(),
            label = "unbounded"
        );

        let (mut txb, mut rxb) = hotpath::channel!(
            futures_channel::mpsc::channel::<i32>(10),
            label = "bounded",
            capacity = 10
        );

        let (txc, rxc) = hotpath::channel!(
            futures_channel::oneshot::channel::<String>(),
            label = "oneshot"
        );

        let sender_handle = smol::spawn(async move {
            for i in 1..=3 {
                println!("[Sender] Sending to unbounded: {}", i);
                txa.unbounded_send(i).expect("Failed to send");
                Timer::after(Duration::from_millis(100)).await;
            }

            for i in 1..=3 {
                println!("[Sender] Sending to bounded: {}", i);
                txb.try_send(i).expect("Failed to send");
                Timer::after(Duration::from_millis(250)).await;
            }

            println!("[Sender] Done sending messages");
        });

        let oneshot_receiver_handle = smol::spawn(async move {
            match rxc.await {
                Ok(msg) => println!("[Oneshot] Received: {}", msg),
                Err(_) => println!("[Oneshot] Sender dropped"),
            }
        });

        println!("[Oneshot] Sending message");
        txc.send("Hello from oneshot!".to_string())
            .expect("Failed to send oneshot");

        sender_handle.await;
        oneshot_receiver_handle.await;

        while let Some(msg) = rxb.next().await {
            println!("[Receiver] Received message: {}", msg);
        }

        println!("\nExample completed!");
    })
}
