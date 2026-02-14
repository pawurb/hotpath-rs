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

        let _channels_guard = hotpath::HotpathGuardBuilder::new("main")
            .with_sections(vec![hotpath::Section::Channels])
            .build();

        let (txa, mut _rxa) = hotpath::channel!(
            futures_channel::mpsc::unbounded::<i32>(),
            label = _actor1.name
        );

        let (mut txb, mut rxb) = hotpath::channel!(
            futures_channel::mpsc::channel::<i32>(10),
            capacity = 10,
            label = "bounded-channel"
        );

        let (txc, rxc) = hotpath::channel!(
            futures_channel::oneshot::channel::<String>(),
            label = "oneshot-labeled"
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
        txc.send("Hello from futures oneshot!".to_string())
            .expect("Failed to send oneshot");

        sender_handle.await;
        oneshot_receiver_handle.await;

        while let Some(msg) = rxb.next().await {
            println!("[Receiver] Received from bounded: {}", msg);
        }

        if let Ok(secs) = std::env::var("TEST_SLEEP_SECONDS") {
            if let Ok(duration) = secs.parse::<u64>() {
                std::thread::sleep(std::time::Duration::from_secs(duration));
            }
        }

        println!("\nFutures channel example completed!");
    })
}
