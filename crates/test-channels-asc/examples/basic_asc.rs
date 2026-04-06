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
            .sections(vec![hotpath::Section::Channels])
            .build();

        let (txa, mut _rxa) =
            hotpath::channel!(async_channel::unbounded::<i32>(), label = _actor1.name);

        let (mut txb, mut rxb) =
            hotpath::channel!(async_channel::bounded::<i32>(10), label = "bounded-channel");

        let sender_handle = smol::spawn(async move {
            for i in 1..=3 {
                println!("[Sender] Sending to unbounded: {}", i);
                txa.send(i).await.expect("Failed to send");
                Timer::after(Duration::from_millis(100)).await;
            }

            for i in 1..=3 {
                println!("[Sender] Sending to bounded: {}", i);
                txb.send(i).await.expect("Failed to send");
                Timer::after(Duration::from_millis(250)).await;
            }

            println!("[Sender] Done sending messages");
        });

        sender_handle.await;

        while let Ok(msg) = rxb.recv().await {
            println!("[Receiver] Received message: {}", msg);
        }

        if let Ok(secs) = std::env::var("TEST_SLEEP_SECONDS") {
            if let Ok(duration) = secs.parse::<u64>() {
                Timer::after(Duration::from_secs(duration)).await;
            }
        }

        println!("\nAsync channel example completed!");
    })
}
