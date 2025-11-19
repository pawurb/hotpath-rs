use std::thread;
use std::time::Duration;

#[allow(unused_mut)]
fn main() {
    #[cfg(feature = "hotpath")]
    let _channels_guard = hotpath::channels::ChannelsGuard::new();

    println!("Open the TUI console to watch live updates!");
    println!("   Run: cargo run -p channels-console --features tui -- console\n");
    thread::sleep(Duration::from_secs(2));

    // Channel 1: Fast data stream - unbounded, rapid messages
    let (tx_fast, rx_fast) = crossbeam_channel::unbounded::<String>();
    #[cfg(feature = "hotpath")]
    let (tx_fast, rx_fast) =
        hotpath::channel!((tx_fast, rx_fast), label = "fast-stream", log = true);

    // Channel 2: Slow consumer - bounded(5), will back up!
    let (tx_slow, rx_slow) = crossbeam_channel::bounded::<String>(5);
    #[cfg(feature = "hotpath")]
    let (tx_slow, rx_slow) = hotpath::channel!(
        (tx_slow, rx_slow),
        label = "slow-consumer",
        capacity = 5,
        log = true
    );

    // Channel 3: Burst traffic - bounded(10), bursts every 3 seconds
    let (tx_burst, rx_burst) = crossbeam_channel::bounded::<u64>(10);
    #[cfg(feature = "hotpath")]
    let (tx_burst, rx_burst) = hotpath::channel!(
        (tx_burst, rx_burst),
        label = "burst-traffic",
        capacity = 10,
        log = true
    );

    // Channel 4: Gradual flow - bounded(20), increasing rate
    let (tx_gradual, rx_gradual) = crossbeam_channel::bounded::<f64>(20);
    #[cfg(feature = "hotpath")]
    let (tx_gradual, rx_gradual) = hotpath::channel!(
        (tx_gradual, rx_gradual),
        label = "gradual-flow",
        capacity = 20,
        log = true
    );

    // Channel 5: Dropped early - unbounded, producer dies at 10s
    let (tx_drop_early, rx_drop_early) = crossbeam_channel::unbounded::<bool>();
    #[cfg(feature = "hotpath")]
    let (tx_drop_early, rx_drop_early) = hotpath::channel!(
        (tx_drop_early, rx_drop_early),
        label = "dropped-early",
        log = true
    );

    // Channel 6: Consumer dies - bounded(8), consumer stops at 15s
    let (tx_consumer_dies, rx_consumer_dies) = crossbeam_channel::bounded::<Vec<u8>>(8);
    #[cfg(feature = "hotpath")]
    let (tx_consumer_dies, rx_consumer_dies) = hotpath::channel!(
        (tx_consumer_dies, rx_consumer_dies),
        label = "consumer-dies",
        capacity = 8,
        log = true
    );

    // Channel 7: Steady stream - unbounded, consistent 500ms rate
    let (tx_steady, rx_steady) = crossbeam_channel::unbounded::<&str>();
    #[cfg(feature = "hotpath")]
    let (tx_steady, rx_steady) =
        hotpath::channel!((tx_steady, rx_steady), label = "steady-stream", log = true);

    println!("Creating 3 bounded iter channels...");
    for i in 0..3 {
        let (tx, rx) = crossbeam_channel::bounded::<u32>(5);

        #[cfg(feature = "hotpath")]
        let (tx, rx) = hotpath::channel!((tx, rx), capacity = 5);

        thread::spawn(move || {
            for j in 0..5 {
                let _ = tx.send(i * 10 + j);
                thread::sleep(Duration::from_millis(500));
            }
        });

        thread::spawn(move || {
            while let Ok(_msg) = rx.recv() {
                thread::sleep(Duration::from_millis(200));
            }
        });
    }

    // === Task 1: Fast data stream producer (10ms interval) ===
    thread::spawn(move || {
        let messages = ["foo", "baz", "bar"];
        for i in 0..3000 {
            let msg = messages[i % messages.len()].to_string();
            if tx_fast.send(msg).is_err() {
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }
    });

    // === Task 2: Fast data stream consumer ===
    thread::spawn(move || {
        while let Ok(_msg) = rx_fast.recv() {
            thread::sleep(Duration::from_millis(15));
        }
    });

    // === Task 3: Slow consumer producer (fast sends) ===
    thread::spawn(move || {
        for i in 0..200 {
            if tx_slow.try_send(format!("MSG-{}", i)).is_err() {
                // Channel full, wait a bit
                thread::sleep(Duration::from_millis(10));
                if tx_slow.try_send(format!("MSG-{}", i)).is_err() {
                    break;
                }
            }
            thread::sleep(Duration::from_millis(100));
        }
    });

    // === Task 4: Slow consumer (very slow, queue backs up!) ===
    thread::spawn(move || {
        while let Ok(msg) = rx_slow.recv() {
            println!("Slow consumer processing: {}", msg);
            thread::sleep(Duration::from_millis(800)); // Much slower than producer!
        }
    });

    // === Task 5: Burst traffic producer ===
    thread::spawn(move || {
        for burst_num in 0..10 {
            println!("Burst #{} starting!", burst_num + 1);
            // Send burst of 15 messages
            for i in 0..15 {
                if tx_burst.try_send(burst_num * 1000 + i).is_err() {
                    // Channel full, wait and retry
                    thread::sleep(Duration::from_millis(50));
                    if tx_burst.try_send(burst_num * 1000 + i).is_err() {
                        return;
                    }
                }
            }
            thread::sleep(Duration::from_secs(3));
        }
    });

    // === Task 6: Burst traffic consumer ===
    thread::spawn(move || {
        while let Ok(_msg) = rx_burst.recv() {
            thread::sleep(Duration::from_millis(200));
        }
    });

    // === Task 7: Gradual flow producer (accelerating rate) ===
    thread::spawn(move || {
        for i in 0..100 {
            if tx_gradual
                .try_send(i as f64 * std::f64::consts::PI)
                .is_err()
            {
                break;
            }
            // Delay decreases over time (speeds up)
            let delay = 500 - (i * 4).min(400);
            thread::sleep(Duration::from_millis(delay));
        }
    });

    // === Task 8: Gradual flow consumer ===
    thread::spawn(move || {
        while rx_gradual.recv().is_ok() {
            thread::sleep(Duration::from_millis(200));
        }
    });

    // === Task 9: Dropped early producer (dies at 10s) ===
    thread::spawn(move || {
        for i in 0..100 {
            if i == 50 {
                println!("'dropped-early' producer dying at 10s!");
                break;
            }
            let _ = tx_drop_early.send(i % 2 == 0);
            thread::sleep(Duration::from_millis(200));
        }
    });

    // === Task 10: Dropped early consumer ===
    thread::spawn(move || {
        while rx_drop_early.recv().is_ok() {
            thread::sleep(Duration::from_millis(100));
        }
        println!("'dropped-early' consumer detected channel closed");
    });

    // === Task 11: Consumer dies producer ===
    thread::spawn(move || {
        for i in 0..300 {
            if tx_consumer_dies.try_send(vec![i as u8; 10]).is_err() {
                println!("'consumer-dies' producer detected closed channel");
                break;
            }
            thread::sleep(Duration::from_millis(100));
        }
    });

    // === Task 12: Consumer dies consumer (dies at 15s) ===
    thread::spawn(move || {
        for _ in 0..75 {
            if rx_consumer_dies.recv().is_ok() {
                thread::sleep(Duration::from_millis(200));
            }
        }
        println!("'consumer-dies' consumer stopping at 15s!");
        drop(rx_consumer_dies);
    });

    // === Task 13: Steady stream producer ===
    thread::spawn(move || {
        let messages = ["tick", "tock", "ping", "pong", "beep", "boop"];
        for i in 0..60 {
            let _ = tx_steady.send(messages[i % messages.len()]);
            thread::sleep(Duration::from_millis(500));
        }
    });

    // === Task 14: Steady stream consumer ===
    thread::spawn(move || {
        while let Ok(_msg) = rx_steady.recv() {
            thread::sleep(Duration::from_millis(400));
        }
    });

    // === Task 15: Timer (progress indicator) ===
    thread::spawn(move || {
        for i in 0..=60 {
            println!("Time: {}s / 60s", i);
            thread::sleep(Duration::from_secs(1));
        }
    });

    thread::sleep(Duration::from_secs(60));
}
