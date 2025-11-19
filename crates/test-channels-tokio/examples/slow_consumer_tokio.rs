use tokio::time::{sleep, Duration};

#[allow(unused_mut)]
#[tokio::main]
async fn main() {
    #[cfg(feature = "hotpath")]
    let _channels_guard = hotpath::channels::ChannelsGuard::new();

    println!("Slow Consumer Example:");
    println!("- Bounded channel with capacity 10");
    println!("- Producer sends 1 message every 10ms");
    println!("- Consumer processes 1 message every 20ms");
    println!("- Queue will back up!\n");

    let (tx, mut rx) = tokio::sync::mpsc::channel::<i32>(10);
    #[cfg(feature = "hotpath")]
    let (tx, mut rx) =
        hotpath::channel!((tx, rx), capacity = 10, label = "slow-consumer", log = true);

    // Producer: sends every 100ms
    let producer_handle = tokio::spawn(async move {
        for i in 1..=50 {
            println!("[Producer] Sending message {}", i);

            // Retry loop with timeout
            let start = std::time::Instant::now();
            loop {
                match tx.try_send(i) {
                    Ok(_) => break,
                    Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => {
                        panic!("[Producer] Channel disconnected");
                    }
                    Err(tokio::sync::mpsc::error::TrySendError::Full(_)) => {
                        // Channel is full, check timeout
                        if start.elapsed() > Duration::from_secs(3) {
                            panic!("[Producer] Send timeout after 3 seconds for message {}", i);
                        }
                        sleep(Duration::from_millis(10)).await;
                    }
                }
            }

            sleep(Duration::from_millis(10)).await;
        }
        println!("[Producer] Done sending messages");
    });

    // Consumer: processes every 20ms (slower than producer!)
    let consumer_handle = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            println!("[Consumer] Processing message: {}", msg);
            sleep(Duration::from_millis(20)).await;
        }
        println!("[Consumer] Channel closed");
    });

    producer_handle.await.unwrap();
    consumer_handle.await.unwrap();

    println!("\nSlow consumer example completed!");
}
