use std::thread;
use std::time::Duration;

#[allow(unused_mut)]
fn main() {
    #[cfg(feature = "hotpath")]
    let _channels_guard = hotpath::channels::ChannelsGuard::new();

    println!("Slow Consumer Example:");
    println!("- Bounded channel with capacity 10");
    println!("- Producer sends 1 message every 10ms");
    println!("- Consumer processes 1 message every 20ms");
    println!("- Queue will back up!\n");

    let (tx, rx) = std::sync::mpsc::sync_channel::<i32>(10);
    #[cfg(feature = "hotpath")]
    let (tx, rx) = hotpath::channel!((tx, rx), capacity = 10, label = "slow-consumer", log = true);

    // Producer: sends every 10ms
    let producer_handle = thread::spawn(move || {
        for i in 1..=50 {
            println!("[Producer] Sending message {}", i);

            // Retry loop with timeout
            let start = std::time::Instant::now();
            loop {
                match tx.try_send(i) {
                    Ok(_) => break,
                    Err(std::sync::mpsc::TrySendError::Disconnected(_)) => {
                        panic!("[Producer] Channel disconnected");
                    }
                    Err(std::sync::mpsc::TrySendError::Full(_)) => {
                        // Channel is full, check timeout
                        if start.elapsed() > Duration::from_secs(2) {
                            panic!("[Producer] Send timeout after 2 seconds for message {}", i);
                        }
                        thread::sleep(Duration::from_millis(5));
                    }
                }
            }

            thread::sleep(Duration::from_millis(5));
        }
        println!("[Producer] Done sending messages");
    });

    // Consumer: processes every 20ms (slower than producer!)
    let consumer_handle = thread::spawn(move || {
        while let Ok(msg) = rx.recv() {
            println!("[Consumer] Processing message: {}", msg);
            thread::sleep(Duration::from_millis(20));
        }
        println!("[Consumer] Channel closed");
    });

    producer_handle.join().unwrap();
    drop(consumer_handle);

    println!("\nSlow consumer example completed!");
}
