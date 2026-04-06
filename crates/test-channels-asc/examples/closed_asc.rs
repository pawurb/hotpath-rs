use smol::Timer;
use std::time::Duration;

#[allow(unused_mut)]
fn main() {
    smol::block_on(async {
        let _channels_guard = hotpath::HotpathGuardBuilder::new("main")
            .sections(vec![hotpath::Section::Channels])
            .build();

        let (txa, mut rxa) =
            hotpath::channel!(async_channel::unbounded::<i32>(), label = "unbounded");

        let (mut txb, mut rxb) =
            hotpath::channel!(async_channel::bounded::<i32>(10), label = "bounded");

        println!("[Unbounded] Sending 3 messages...");
        for i in 1..=3 {
            txa.send(i).await.expect("Failed to send");
        }

        for _ in 0..3 {
            if let Ok(msg) = rxa.recv().await {
                println!("[Unbounded] Received: {}", msg);
            }
        }

        println!("[Bounded] Sending 3 messages...");
        for i in 1..=3 {
            txb.send(i).await.expect("Failed to send");
        }

        for _ in 0..3 {
            if let Ok(msg) = rxb.recv().await {
                println!("[Bounded] Received: {}", msg);
            }
        }

        println!("\nClosing channels from receiver side...");

        drop(rxa);
        println!("[Unbounded] Receiver closed");

        drop(rxb);
        println!("[Bounded] Receiver closed");

        Timer::after(Duration::from_millis(100)).await;

        println!("\nAttempting to send after closing receivers...");

        match txa.send(999).await {
            Ok(_) => println!("[Unbounded] Send succeeded (buffered, receiver already closed)"),
            Err(_) => println!("[Unbounded] Send failed - channel closed"),
        }

        match txb.send(999).await {
            Ok(_) => println!("[Bounded] Send succeeded (unexpected)"),
            Err(_) => println!("[Bounded] Send failed - channel closed"),
        }

        Timer::after(Duration::from_millis(100)).await;

        println!("\nExample completed!");
    })
}
