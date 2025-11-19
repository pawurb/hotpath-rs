use futures_util::stream::StreamExt;
use smol::Timer;
use std::time::Duration;

#[allow(unused_mut)]
fn main() {
    smol::block_on(async {
        #[cfg(feature = "hotpath")]
        let _channels_guard = hotpath::channels::ChannelsGuard::new();

        println!("Slow Consumer Example:");
        println!("- Bounded channel with capacity 10");
        println!("- Producer sends 1 message every 10ms");
        println!("- Consumer processes 1 message every 20ms");
        println!("- Queue will back up!\n");

        let (mut tx, mut rx) = futures_channel::mpsc::channel::<i32>(10);
        #[cfg(feature = "hotpath")]
        let (mut tx, mut rx) =
            hotpath::channel!((tx, rx), capacity = 10, label = "slow-consumer", log = true);

        // Producer: sends every 10ms
        let producer_handle = smol::spawn(async move {
            for i in 1..=50 {
                println!("[Producer] Sending message {}", i);

                // Retry loop with timeout
                let start = std::time::Instant::now();
                loop {
                    match tx.try_send(i) {
                        Ok(_) => break,
                        Err(e) if e.is_disconnected() => {
                            panic!("[Producer] Channel disconnected: {:?}", e);
                        }
                        Err(_) => {
                            // Channel is full, check timeout
                            if start.elapsed() > Duration::from_secs(2) {
                                panic!("[Producer] Send timeout after 2 seconds for message {}", i);
                            }
                            Timer::after(Duration::from_millis(10)).await;
                        }
                    }
                }

                Timer::after(Duration::from_millis(10)).await;
            }
            println!("[Producer] Done sending messages");
        });

        // Consumer: processes every 20ms (slower than producer!)
        let consumer_handle = smol::spawn(async move {
            while let Some(msg) = rx.next().await {
                println!("[Consumer] Processing message: {}", msg);
                Timer::after(Duration::from_millis(20)).await;
            }
            println!("[Consumer] Channel closed");
        });

        producer_handle.await;
        consumer_handle.await;

        println!("\nSlow consumer example completed!");
    })
}
