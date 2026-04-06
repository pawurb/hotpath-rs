use smol::Timer;
use std::time::Duration;

#[allow(unused_mut)]
fn main() {
    smol::block_on(async {
        let _channels_guard = hotpath::HotpathGuardBuilder::new("main")
            .format(hotpath::Format::JsonPretty)
            .sections(vec![hotpath::Section::Channels])
            .build();

        let (txa, mut _rxa) =
            hotpath::channel!(async_channel::unbounded::<i32>(), label = "unbounded");

        let (mut txb, mut rxb) =
            hotpath::channel!(async_channel::bounded::<i32>(10), label = "bounded");

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

        println!("\nExample completed!");
    })
}
