use futures_util::stream::StreamExt;
use smol::Timer;
use std::time::Duration;

#[allow(unused_mut)]
fn main() {
    smol::block_on(async {
        #[cfg(feature = "hotpath")]
        let _channels_guard = hotpath::channels::ChannelsGuard::new();

        println!("Open the TUI console to watch live updates!");
        println!("   Run: cargo run -p channels-console --features tui -- console\n");
        Timer::after(Duration::from_secs(2)).await;

        // Channel 1: Fast data stream - unbounded, rapid messages
        let (tx_fast, mut rx_fast) = futures_channel::mpsc::unbounded::<String>();
        #[cfg(feature = "hotpath")]
        let (tx_fast, mut rx_fast) =
            hotpath::channel!((tx_fast, rx_fast), label = "fast-stream", log = true);

        // Channel 2: Slow consumer - bounded(5), will back up!
        let (mut tx_slow, mut rx_slow) = futures_channel::mpsc::channel::<String>(5);
        #[cfg(feature = "hotpath")]
        let (mut tx_slow, mut rx_slow) = hotpath::channel!(
            (tx_slow, rx_slow),
            label = "slow-consumer",
            capacity = 5,
            log = true
        );

        // Channel 3: Burst traffic - bounded(10), bursts every 3 seconds
        let (mut tx_burst, mut rx_burst) = futures_channel::mpsc::channel::<u64>(10);
        #[cfg(feature = "hotpath")]
        let (mut tx_burst, mut rx_burst) = hotpath::channel!(
            (tx_burst, rx_burst),
            label = "burst-traffic",
            capacity = 10,
            log = true
        );

        // Channel 4: Gradual flow - bounded(20), increasing rate
        let (mut tx_gradual, mut rx_gradual) = futures_channel::mpsc::channel::<f64>(20);
        #[cfg(feature = "hotpath")]
        let (mut tx_gradual, mut rx_gradual) = hotpath::channel!(
            (tx_gradual, rx_gradual),
            label = "gradual-flow",
            capacity = 20,
            log = true
        );

        // Channel 5: Dropped early - unbounded, producer dies at 10s
        let (tx_drop_early, mut rx_drop_early) = futures_channel::mpsc::unbounded::<bool>();
        #[cfg(feature = "hotpath")]
        let (tx_drop_early, mut rx_drop_early) = hotpath::channel!(
            (tx_drop_early, rx_drop_early),
            label = "dropped-early",
            log = true
        );

        // Channel 6: Consumer dies - bounded(8), consumer stops at 15s
        let (mut tx_consumer_dies, mut rx_consumer_dies) =
            futures_channel::mpsc::channel::<Vec<u8>>(8);
        #[cfg(feature = "hotpath")]
        let (mut tx_consumer_dies, mut rx_consumer_dies) = hotpath::channel!(
            (tx_consumer_dies, rx_consumer_dies),
            label = "consumer-dies",
            capacity = 8,
            log = true
        );

        // Channel 7: Steady stream - unbounded, consistent 500ms rate
        let (tx_steady, mut rx_steady) = futures_channel::mpsc::unbounded::<&str>();
        #[cfg(feature = "hotpath")]
        let (tx_steady, mut rx_steady) =
            hotpath::channel!((tx_steady, rx_steady), label = "steady-stream", log = true);

        // Channel 8: Oneshot early - fires at 5 seconds
        let (tx_oneshot_early, rx_oneshot_early) = futures_channel::oneshot::channel::<String>();
        #[cfg(feature = "hotpath")]
        let (tx_oneshot_early, rx_oneshot_early) = hotpath::channel!(
            (tx_oneshot_early, rx_oneshot_early),
            label = "oneshot-early",
            log = true
        );

        // Channel 9: Oneshot mid - fires at 15 seconds
        let (tx_oneshot_mid, rx_oneshot_mid) = futures_channel::oneshot::channel::<u32>();
        #[cfg(feature = "hotpath")]
        let (tx_oneshot_mid, rx_oneshot_mid) = hotpath::channel!(
            (tx_oneshot_mid, rx_oneshot_mid),
            label = "oneshot-mid",
            log = true
        );

        // Channel 10: Oneshot late - fires at 25 seconds
        let (tx_oneshot_late, rx_oneshot_late) = futures_channel::oneshot::channel::<i64>();
        #[cfg(feature = "hotpath")]
        let (tx_oneshot_late, rx_oneshot_late) = hotpath::channel!(
            (tx_oneshot_late, rx_oneshot_late),
            label = "oneshot-late",
            log = true
        );

        println!("Creating 3 bounded iter channels...");
        for i in 0..3 {
            let (mut tx, mut rx) = futures_channel::mpsc::channel::<u32>(5);

            #[cfg(feature = "hotpath")]
            let (mut tx, mut rx) = hotpath::channel!((tx, rx), capacity = 5);

            smol::spawn(async move {
                for j in 0..5 {
                    let _ = tx.try_send(i * 10 + j);
                    Timer::after(Duration::from_millis(500)).await;
                }
            })
            .detach();

            smol::spawn(async move {
                while let Some(_msg) = rx.next().await {
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
                if tx_fast.unbounded_send(msg).is_err() {
                    break;
                }
                Timer::after(Duration::from_millis(10)).await;
            }
        })
        .detach();

        // === Task 2: Fast data stream consumer ===
        smol::spawn(async move {
            while let Some(msg) = rx_fast.next().await {
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
            while let Some(msg) = rx_slow.next().await {
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
            while let Some(msg) = rx_burst.next().await {
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
            while rx_gradual.next().await.is_some() {
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
                let _ = tx_drop_early.unbounded_send(i % 2 == 0);
                Timer::after(Duration::from_millis(200)).await;
            }
        })
        .detach();

        // === Task 10: Dropped early consumer ===
        smol::spawn(async move {
            while rx_drop_early.next().await.is_some() {
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
                if rx_consumer_dies.next().await.is_some() {
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
                let _ = tx_steady.unbounded_send(messages[i % messages.len()]);
                Timer::after(Duration::from_millis(500)).await;
            }
        })
        .detach();

        // === Task 14: Steady stream consumer ===
        smol::spawn(async move {
            while let Some(msg) = rx_steady.next().await {
                let _ = msg;
                Timer::after(Duration::from_millis(400)).await;
            }
        })
        .detach();

        // === Task 15: Oneshot receivers ===
        smol::spawn(async move {
            match rx_oneshot_early.await {
                Ok(msg) => println!("Oneshot-early received: {}", msg),
                Err(_) => println!("Oneshot-early sender dropped"),
            }
        })
        .detach();

        smol::spawn(async move {
            match rx_oneshot_mid.await {
                Ok(msg) => println!("Oneshot-mid received: {}", msg),
                Err(_) => println!("Oneshot-mid sender dropped"),
            }
        })
        .detach();

        smol::spawn(async move {
            match rx_oneshot_late.await {
                Ok(msg) => println!("Oneshot-late received: {}", msg),
                Err(_) => println!("Oneshot-late sender dropped"),
            }
        })
        .detach();

        // === Task 16: Oneshot senders (fire at specific times) ===
        smol::spawn(async move {
            Timer::after(Duration::from_secs(5)).await;
            println!("Firing oneshot-early at 5s");
            let _ = tx_oneshot_early.send("Early bird gets the worm!".to_string());
        })
        .detach();

        smol::spawn(async move {
            Timer::after(Duration::from_secs(15)).await;
            println!("Firing oneshot-mid at 15s");
            let _ = tx_oneshot_mid.send(42);
        })
        .detach();

        smol::spawn(async move {
            Timer::after(Duration::from_secs(25)).await;
            println!("Firing oneshot-late at 25s");
            let _ = tx_oneshot_late.send(9000);
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
