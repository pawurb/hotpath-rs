use smol::Timer;
use std::time::Duration;

#[allow(unused_mut)]
fn main() {
    smol::block_on(async {
        let _channels_guard = hotpath::HotpathGuardBuilder::new("main")
            .sections(vec![hotpath::Section::Channels])
            .build();

        println!("Open the TUI console to watch live updates!");
        println!("   Run: cargo run -p channels-console --features tui -- console\n");
        Timer::after(Duration::from_secs(2)).await;

        // Channel 1: Fast data stream - unbounded, rapid messages
        let (tx_fast, mut rx_fast) = hotpath::channel!(
            flume::unbounded::<String>(),
            label = "fast-stream",
            log = true
        );

        // Channel 2: Slow consumer - bounded(5), will back up!
        let (mut tx_slow, mut rx_slow) = hotpath::channel!(
            flume::bounded::<String>(5),
            label = "slow-consumer",
            log = true
        );

        // Channel 3: Burst traffic - bounded(10), bursts every 3 seconds
        let (mut tx_burst, mut rx_burst) = hotpath::channel!(
            flume::bounded::<u64>(10),
            label = "burst-traffic",
            log = true
        );

        // Channel 4: Gradual flow - bounded(20), increasing rate
        let (mut tx_gradual, mut rx_gradual) = hotpath::channel!(
            flume::bounded::<f64>(20),
            label = "gradual-flow",
            log = true
        );

        // Channel 5: Dropped early - unbounded, producer dies at 10s
        let (tx_drop_early, mut rx_drop_early) = hotpath::channel!(
            flume::unbounded::<bool>(),
            label = "dropped-early",
            log = true
        );

        // Channel 6: Consumer dies - bounded(8), consumer stops at 15s
        let (mut tx_consumer_dies, mut rx_consumer_dies) = hotpath::channel!(
            flume::bounded::<Vec<u8>>(8),
            label = "consumer-dies",
            log = true
        );

        // Channel 7: Steady stream - unbounded, consistent 500ms rate
        let (tx_steady, mut rx_steady) = hotpath::channel!(
            flume::unbounded::<&str>(),
            label = "steady-stream",
            log = true
        );

        println!("Creating 3 bounded iter channels...");
        for i in 0..3 {
            let (mut tx, mut rx) = hotpath::channel!(flume::bounded::<u32>(5));

            smol::spawn(async move {
                for j in 0..5 {
                    let _ = tx.try_send(i * 10 + j);
                    Timer::after(Duration::from_millis(500)).await;
                }
            })
            .detach();

            smol::spawn(async move {
                while let Ok(_msg) = rx.recv_async().await {
                    Timer::after(Duration::from_millis(200)).await;
                }
            })
            .detach();
        }

        // === Task 1: Fast data stream producer (10ms interval) ===
        smol::spawn(async move {
            let messages = ["foo", "baz", "bar"];
            for i in 0..3000 {
                let msg = messages[i % messages.len()].to_string();
                if tx_fast.try_send(msg).is_err() {
                    break;
                }
                Timer::after(Duration::from_millis(10)).await;
            }
        })
        .detach();

        // === Task 2: Fast data stream consumer ===
        smol::spawn(async move {
            while let Ok(msg) = rx_fast.recv_async().await {
                let _ = msg;
                Timer::after(Duration::from_millis(15)).await;
            }
        })
        .detach();

        // === Task 3: Slow consumer producer (fast sends) ===
        smol::spawn(async move {
            for i in 0..200 {
                if tx_slow.try_send(format!("MSG-{}", i)).is_err() {
                    // Channel full, wait a bit
                    Timer::after(Duration::from_millis(10)).await;
                    if tx_slow.try_send(format!("MSG-{}", i)).is_err() {
                        break;
                    }
                }
                Timer::after(Duration::from_millis(100)).await;
            }
        })
        .detach();

        // === Task 4: Slow consumer (very slow, queue backs up!) ===
        smol::spawn(async move {
            while let Ok(msg) = rx_slow.recv_async().await {
                println!("Slow consumer processing: {}", msg);
                Timer::after(Duration::from_millis(800)).await; // Much slower than producer!
            }
        })
        .detach();

        // === Task 5: Burst traffic producer ===
        smol::spawn(async move {
            for burst_num in 0..10 {
                println!("Burst #{} starting!", burst_num + 1);
                // Send burst of 15 messages
                for i in 0..15 {
                    if tx_burst.try_send(burst_num * 1000 + i).is_err() {
                        // Channel full, wait and retry
                        Timer::after(Duration::from_millis(50)).await;
                        if tx_burst.try_send(burst_num * 1000 + i).is_err() {
                            return;
                        }
                    }
                }
                Timer::after(Duration::from_secs(3)).await;
            }
        })
        .detach();

        // === Task 6: Burst traffic consumer ===
        smol::spawn(async move {
            while let Ok(msg) = rx_burst.recv_async().await {
                let _ = msg;
                Timer::after(Duration::from_millis(200)).await;
            }
        })
        .detach();

        // === Task 7: Gradual flow producer (accelerating rate) ===
        smol::spawn(async move {
            for i in 0..100 {
                if tx_gradual
                    .try_send(i as f64 * std::f64::consts::PI)
                    .is_err()
                {
                    break;
                }
                // Delay decreases over time (speeds up)
                let delay = 500 - (i * 4).min(400);
                Timer::after(Duration::from_millis(delay)).await;
            }
        })
        .detach();

        // === Task 8: Gradual flow consumer ===
        smol::spawn(async move {
            while rx_gradual.recv_async().await.is_ok() {
                Timer::after(Duration::from_millis(200)).await;
            }
        })
        .detach();

        // === Task 9: Dropped early producer (dies at 10s) ===
        smol::spawn(async move {
            for i in 0..100 {
                if i == 50 {
                    println!("'dropped-early' producer dying at 10s!");
                    break;
                }
                let _ = tx_drop_early.try_send(i % 2 == 0);
                Timer::after(Duration::from_millis(200)).await;
            }
        })
        .detach();

        // === Task 10: Dropped early consumer ===
        smol::spawn(async move {
            while rx_drop_early.recv_async().await.is_ok() {
                Timer::after(Duration::from_millis(100)).await;
            }
            println!("'dropped-early' consumer detected channel closed");
        })
        .detach();

        // === Task 11: Consumer dies producer ===
        smol::spawn(async move {
            for i in 0..300 {
                if tx_consumer_dies.try_send(vec![i as u8; 10]).is_err() {
                    println!("'consumer-dies' producer detected closed channel");
                    break;
                }
                Timer::after(Duration::from_millis(100)).await;
            }
        })
        .detach();

        // === Task 12: Consumer dies consumer (dies at 15s) ===
        smol::spawn(async move {
            for _ in 0..75 {
                if rx_consumer_dies.recv_async().await.is_ok() {
                    Timer::after(Duration::from_millis(200)).await;
                }
            }
            println!("'consumer-dies' consumer stopping at 15s!");
            drop(rx_consumer_dies);
        })
        .detach();

        // === Task 13: Steady stream producer ===
        smol::spawn(async move {
            let messages = ["tick", "tock", "ping", "pong", "beep", "boop"];
            for i in 0..60 {
                let _ = tx_steady.try_send(messages[i % messages.len()]);
                Timer::after(Duration::from_millis(500)).await;
            }
        })
        .detach();

        // === Task 14: Steady stream consumer ===
        smol::spawn(async move {
            while let Ok(msg) = rx_steady.recv_async().await {
                let _ = msg;
                Timer::after(Duration::from_millis(400)).await;
            }
        })
        .detach();

        smol::spawn(async move {
            for i in 0..=60 {
                println!("Time: {}s / 60s", i);
                Timer::after(Duration::from_secs(1)).await;
            }
        })
        .detach();

        Timer::after(Duration::from_secs(60)).await;
    })
}
