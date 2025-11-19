use futures_util::stream::StreamExt;
use smol::Timer;
use std::time::Duration;

#[allow(dead_code)]
struct Actor {
    name: String,
}

#[allow(unused_mut)]
fn main() {
    smol::block_on(async {
        let _actor1 = Actor {
            name: "Actor 1".to_string(),
        };

        #[cfg(feature = "hotpath")]
        let _channels_guard = hotpath::channels::ChannelsGuard::new();

        let (txa, mut _rxa) = futures_channel::mpsc::unbounded::<i32>();
        #[cfg(feature = "hotpath")]
        let (txa, mut _rxa) = hotpath::channel!((txa, _rxa), label = _actor1.name);

        let (mut txb, mut rxb) = futures_channel::mpsc::channel::<i32>(10);
        #[cfg(feature = "hotpath")]
        let (mut txb, mut rxb) =
            hotpath::channel!((txb, rxb), capacity = 10, label = "bounded-channel");

        let (txc, rxc) = futures_channel::oneshot::channel::<String>();
        #[cfg(feature = "hotpath")]
        let (txc, rxc) = hotpath::channel!((txc, rxc), label = "oneshot-labeled");

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
        txc.send("Hello from futures oneshot!".to_string())
            .expect("Failed to send oneshot");

        sender_handle.await;
        oneshot_receiver_handle.await;

        while let Some(msg) = rxb.next().await {
            println!("[Receiver] Received from bounded: {}", msg);
        }

        println!("\nFutures channel example completed!");
    })
}
