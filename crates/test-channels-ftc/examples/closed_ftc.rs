use futures_util::stream::StreamExt;
use smol::Timer;
use std::time::Duration;

#[allow(unused_mut)]
fn main() {
    smol::block_on(async {
        let _channels_guard = hotpath::HotpathGuardBuilder::new("main")
            .with_sections(vec![hotpath::Section::Channels])
            .build();

        let (txa, mut rxa) = hotpath::channel!(
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

        println!("[Unbounded] Sending 3 messages...");
        for i in 1..=3 {
            txa.unbounded_send(i).expect("Failed to send");
        }

        for _ in 0..3 {
            if let Some(msg) = rxa.next().await {
                println!("[Unbounded] Received: {}", msg);
            }
        }

        println!("[Bounded] Sending 3 messages...");
        for i in 1..=3 {
            txb.try_send(i).expect("Failed to send");
        }

        for _ in 0..3 {
            if let Some(msg) = rxb.next().await {
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

        Timer::after(Duration::from_millis(100)).await;

        println!("\nAttempting to send after closing receivers...");

        match txa.unbounded_send(999) {
            Ok(_) => println!("[Unbounded] Send succeeded (buffered, receiver already closed)"),
            Err(_) => println!("[Unbounded] Send failed - channel closed"),
        }

        match txb.try_send(999) {
            Ok(_) => println!("[Bounded] Send succeeded (unexpected)"),
            Err(_) => println!("[Bounded] Send failed - channel closed"),
        }

        Timer::after(Duration::from_millis(100)).await;

        println!("\nExample completed!");
    })
}
