use tokio::time::{sleep, Duration};

#[allow(unused)]
#[derive(Debug)]
struct UserData {
    id: u64,
    name: String,
    email: String,
    age: u8,
}

impl UserData {
    fn random() -> Self {
        Self {
            id: rand::random(),
            name: format!("User {}", rand::random::<u8>()),
            email: format!("user{}@example.com", rand::random::<u8>()),
            age: rand::random(),
        }
    }
}

#[allow(unused)]
#[derive(Debug)]
struct BigPayload {
    request_id: String,
    timestamp: u64,
    user_id: u64,
    session_id: String,
    ip_address: String,
    user_agent: String,
    request_path: String,
    request_method: String,
    status_code: u16,
    response_time_ms: u64,
    bytes_sent: u64,
    bytes_received: u64,
    error_message: Option<String>,
    metadata: String,
    region: String,
    datacenter: String,
    server_instance: String,
    client_version: String,
    api_version: String,
    correlation_id: String,
    parent_request_id: Option<String>,
}

impl BigPayload {
    fn random() -> Self {
        let has_error = rand::random::<bool>();
        Self {
            request_id: format!("req_{:016x}", rand::random::<u64>()),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
            user_id: rand::random(),
            session_id: format!("sess_{:016x}", rand::random::<u64>()),
            ip_address: format!(
                "{}.{}.{}.{}",
                rand::random::<u8>(),
                rand::random::<u8>(),
                rand::random::<u8>(),
                rand::random::<u8>()
            ),
            user_agent: format!(
                "Mozilla/5.0 (Platform) Browser/{}.{}.{}",
                rand::random::<u8>() % 10,
                rand::random::<u8>() % 20,
                rand::random::<u16>() % 1000
            ),
            request_path: format!(
                "/api/v1/resources/{}/items/{}",
                rand::random::<u16>() % 100,
                rand::random::<u32>()
            ),
            request_method: ["GET", "POST", "PUT", "DELETE", "PATCH"]
                [(rand::random::<u8>() % 5) as usize]
                .to_string(),
            status_code: if has_error {
                [400, 404, 500, 503][(rand::random::<u8>() % 4) as usize]
            } else {
                [200, 201, 204][(rand::random::<u8>() % 3) as usize]
            },
            response_time_ms: rand::random::<u64>() % 5000,
            bytes_sent: rand::random::<u64>() % 1_000_000,
            bytes_received: rand::random::<u64>() % 100_000,
            error_message: if has_error {
                Some(format!(
                    "Error: Something went wrong with code {}",
                    rand::random::<u16>()
                ))
            } else {
                None
            },
            metadata: format!(
                r#"{{"custom_field_1":"value_{}","custom_field_2":{},"tags":["tag1","tag2","tag3"]}}"#,
                rand::random::<u32>(),
                rand::random::<bool>()
            ),
            region: ["us-east-1", "us-west-2", "eu-west-1", "ap-southeast-1"]
                [(rand::random::<u8>() % 4) as usize]
                .to_string(),
            datacenter: format!("dc-{}", rand::random::<u8>() % 10),
            server_instance: format!("i-{:016x}", rand::random::<u64>()),
            client_version: format!(
                "{}.{}.{}",
                rand::random::<u8>() % 5,
                rand::random::<u8>() % 20,
                rand::random::<u8>() % 100
            ),
            api_version: format!("v{}", rand::random::<u8>() % 5 + 1),
            correlation_id: format!("corr_{:016x}", rand::random::<u64>()),
            parent_request_id: if rand::random::<bool>() {
                Some(format!("parent_req_{:016x}", rand::random::<u64>()))
            } else {
                None
            },
        }
    }
}

#[allow(unused_mut)]
#[tokio::main]
async fn main() {
    #[cfg(feature = "tokio-console")]
    console_subscriber::init();

    #[cfg(feature = "hotpath")]
    let _channels_guard = hotpath::channels::ChannelsGuard::new();

    println!("Open the TUI console to watch live updates!");
    println!("   Run: cargo run -p channels-console --features tui -- console\n");
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Channel 1: Fast data stream - unbounded, rapid messages
    let (tx_fast, mut rx_fast) = tokio::sync::mpsc::unbounded_channel::<String>();
    #[cfg(feature = "hotpath")]
    let (tx_fast, mut rx_fast) = hotpath::channel!((tx_fast, rx_fast), label = "fast-data-stream");

    // Channel 2: Slow consumer - bounded(5), will back up!
    let (tx_slow, mut rx_slow) = tokio::sync::mpsc::channel::<UserData>(5);
    #[cfg(feature = "hotpath")]
    let (tx_slow, mut rx_slow) =
        hotpath::channel!((tx_slow, rx_slow), label = "slow-consumer", log = true);

    // Channel 3: Burst traffic - bounded(10), bursts every 3 seconds
    let (tx_burst, mut rx_burst) = tokio::sync::mpsc::channel::<u64>(10);
    #[cfg(feature = "hotpath")]
    let (tx_burst, mut rx_burst) =
        hotpath::channel!((tx_burst, rx_burst), label = "burst-traffic", log = true);

    // Channel 4: Gradual flow - bounded(20), increasing rate
    let (tx_gradual, mut rx_gradual) = tokio::sync::mpsc::channel::<f64>(20);
    #[cfg(feature = "hotpath")]
    let (tx_gradual, mut rx_gradual) = hotpath::channel!((tx_gradual, rx_gradual), log = true);

    // Channel 5: Dropped early - unbounded, producer dies at 10s
    let (tx_drop_early, mut rx_drop_early) = tokio::sync::mpsc::unbounded_channel::<bool>();
    #[cfg(feature = "hotpath")]
    let (tx_drop_early, mut rx_drop_early) =
        hotpath::channel!((tx_drop_early, rx_drop_early), log = true);

    // Channel 6: Consumer dies - bounded(8), consumer stops at 15s
    let (tx_consumer_dies, mut rx_consumer_dies) = tokio::sync::mpsc::channel::<Vec<u8>>(8);
    #[cfg(feature = "hotpath")]
    let (tx_consumer_dies, mut rx_consumer_dies) = hotpath::channel!(
        (tx_consumer_dies, rx_consumer_dies),
        label = "consumer-dies",
        log = true
    );

    // Channel 7: Steady stream - unbounded, consistent 500ms rate
    let (tx_steady, mut rx_steady) = tokio::sync::mpsc::unbounded_channel::<&str>();
    #[cfg(feature = "hotpath")]
    let (tx_steady, mut rx_steady) = hotpath::channel!((tx_steady, rx_steady), log = true);

    // Channel 8: Oneshot early - fires at 5 seconds
    let (tx_oneshot_early, rx_oneshot_early) = tokio::sync::oneshot::channel::<String>();
    #[cfg(feature = "hotpath")]
    let (tx_oneshot_early, rx_oneshot_early) =
        hotpath::channel!((tx_oneshot_early, rx_oneshot_early), log = true);

    // Channel 9: Oneshot mid - fires at 15 seconds
    let (tx_oneshot_mid, rx_oneshot_mid) = tokio::sync::oneshot::channel::<u32>();
    #[cfg(feature = "hotpath")]
    let (tx_oneshot_mid, rx_oneshot_mid) =
        hotpath::channel!((tx_oneshot_mid, rx_oneshot_mid), log = true);

    // Channel 10: Oneshot late - fires at 25 seconds
    let (tx_oneshot_late, rx_oneshot_late) = tokio::sync::oneshot::channel::<i64>();
    #[cfg(feature = "hotpath")]
    let (tx_oneshot_late, rx_oneshot_late) = hotpath::channel!(
        (tx_oneshot_late, rx_oneshot_late),
        label = "oneshot-late",
        log = true
    );

    // Channel 11: Big payload - bounded(10), sends large structured data
    let (tx_big_payload, mut rx_big_payload) = tokio::sync::mpsc::channel::<BigPayload>(10);
    #[cfg(feature = "hotpath")]
    let (tx_big_payload, mut rx_big_payload) = hotpath::channel!(
        (tx_big_payload, rx_big_payload),
        label = "big-payload",
        log = true
    );

    println!("Creating 3 bounded iter channels...");
    for i in 0..3 {
        let (tx, mut rx) = tokio::sync::mpsc::channel::<u32>(5);

        #[cfg(feature = "hotpath")]
        let (tx, mut rx) = hotpath::channel!((tx, rx), log = true);

        tokio::spawn(async move {
            for j in 0..50 {
                let _ = tx.send(i * 10 + j).await;
                sleep(Duration::from_millis(500)).await;
            }
        });

        tokio::spawn(async move {
            while let Some(_msg) = rx.recv().await {
                sleep(Duration::from_millis(200)).await;
            }
        });
    }

    // === Task 1: Fast data stream producer (10ms interval) ===
    tokio::spawn(async move {
        let messages = ["foo", "baz", "bar"];
        for i in 0..3000 {
            let msg = messages[i % messages.len()].to_string();
            if tx_fast.send(msg).is_err() {
                break;
            }
            sleep(Duration::from_millis(10)).await;
        }
    });

    // === Task 2: Fast data stream consumer ===
    tokio::spawn(async move {
        while let Some(msg) = rx_fast.recv().await {
            let _ = msg;
            sleep(Duration::from_millis(15)).await;
        }
    });

    // === Task 3: Slow consumer producer (fast sends) ===
    tokio::spawn(async move {
        for _ in 0..200 {
            if tx_slow.send(UserData::random()).await.is_err() {
                break;
            }
            sleep(Duration::from_millis(100)).await;
        }
    });

    // === Task 4: Slow consumer (very slow, queue backs up!) ===
    tokio::spawn(async move {
        while let Some(msg) = rx_slow.recv().await {
            println!("Slow consumer processing: {:?}", msg);
            sleep(Duration::from_millis(800)).await; // Much slower than producer!
        }
    });

    // === Task 5: Burst traffic producer ===
    tokio::spawn(async move {
        for burst_num in 0..10 {
            println!("Burst #{} starting!", burst_num + 1);
            // Send burst of 15 messages
            for i in 0..15 {
                if tx_burst.send(burst_num * 1000 + i).await.is_err() {
                    return;
                }
            }
            sleep(Duration::from_secs(3)).await;
        }
    });

    // === Task 6: Burst traffic consumer ===
    tokio::spawn(async move {
        while let Some(msg) = rx_burst.recv().await {
            let _ = msg;
            sleep(Duration::from_millis(200)).await;
        }
    });

    // === Task 7: Gradual flow producer (accelerating rate) ===
    tokio::spawn(async move {
        for i in 0..100 {
            if tx_gradual
                .send(i as f64 * std::f64::consts::PI)
                .await
                .is_err()
            {
                break;
            }
            // Delay decreases over time (speeds up)
            let delay = 500 - (i * 4).min(400);
            sleep(Duration::from_millis(delay)).await;
        }
    });

    // === Task 8: Gradual flow consumer ===
    tokio::spawn(async move {
        while rx_gradual.recv().await.is_some() {
            sleep(Duration::from_millis(200)).await;
        }
    });

    // === Task 9: Dropped early producer (dies at 10s) ===
    tokio::spawn(async move {
        for i in 0..100 {
            if i == 50 {
                println!("'dropped-early' producer dying at 10s!");
                break;
            }
            let _ = tx_drop_early.send(i % 2 == 0);
            sleep(Duration::from_millis(200)).await;
        }
    });

    // === Task 10: Dropped early consumer ===
    tokio::spawn(async move {
        while rx_drop_early.recv().await.is_some() {
            sleep(Duration::from_millis(100)).await;
        }
        println!("'dropped-early' consumer detected channel closed");
    });

    // === Task 11: Consumer dies producer ===
    let consumer_dies_handle = tokio::spawn(async move {
        for i in 0..300 {
            if tx_consumer_dies.send(vec![i as u8; 10]).await.is_err() {
                println!("'consumer-dies' producer detected closed channel");
                break;
            }
            sleep(Duration::from_millis(100)).await;
        }
    });

    // === Task 12: Consumer dies consumer (dies at 15s) ===
    tokio::spawn(async move {
        for _ in 0..75 {
            if rx_consumer_dies.recv().await.is_some() {
                sleep(Duration::from_millis(200)).await;
            }
        }
        println!("'consumer-dies' consumer stopping at 15s!");
        drop(rx_consumer_dies);
    });

    // === Task 13: Steady stream producer ===
    tokio::spawn(async move {
        let messages = ["tick", "tock", "ping", "pong", "beep", "boop"];
        for i in 0..60 {
            let _ = tx_steady.send(messages[i % messages.len()]);
            sleep(Duration::from_millis(500)).await;
        }
    });

    // === Task 14: Steady stream consumer ===
    tokio::spawn(async move {
        while let Some(msg) = rx_steady.recv().await {
            let _ = msg;
            sleep(Duration::from_millis(400)).await;
        }
    });

    // === Task 15: Oneshot receivers ===
    tokio::spawn(async move {
        match rx_oneshot_early.await {
            Ok(msg) => println!("Oneshot-early received: {}", msg),
            Err(_) => println!("Oneshot-early sender dropped"),
        }
    });

    tokio::spawn(async move {
        match rx_oneshot_mid.await {
            Ok(msg) => println!("Oneshot-mid received: {}", msg),
            Err(_) => println!("Oneshot-mid sender dropped"),
        }
    });

    tokio::spawn(async move {
        match rx_oneshot_late.await {
            Ok(msg) => println!("Oneshot-late received: {}", msg),
            Err(_) => println!("Oneshot-late sender dropped"),
        }
    });

    // === Task 16: Oneshot senders (fire at specific times) ===
    tokio::spawn(async move {
        sleep(Duration::from_secs(5)).await;
        println!("Firing oneshot-early at 5s");
        let _ = tx_oneshot_early.send("Early bird gets the worm!".to_string());
    });

    tokio::spawn(async move {
        sleep(Duration::from_secs(15)).await;
        println!("Firing oneshot-mid at 15s");
        let _ = tx_oneshot_mid.send(42);
    });

    tokio::spawn(async move {
        sleep(Duration::from_secs(25)).await;
        println!("Firing oneshot-late at 25s");
        let _ = tx_oneshot_late.send(9000);
    });

    // === Task 17: Big payload producer (sends every 2 seconds) ===
    tokio::spawn(async move {
        for i in 0..30 {
            let payload = BigPayload::random();
            println!(
                "Sending big payload #{}: {} to {}",
                i, payload.request_method, payload.request_path
            );
            if tx_big_payload.send(payload).await.is_err() {
                break;
            }
            sleep(Duration::from_secs(2)).await;
        }
    });

    // === Task 18: Big payload consumer (processes every 1.5 seconds) ===
    tokio::spawn(async move {
        while let Some(payload) = rx_big_payload.recv().await {
            println!(
                "Processing big payload: {} {} [status: {}] from {} - {}ms",
                payload.request_method,
                payload.request_path,
                payload.status_code,
                payload.region,
                payload.response_time_ms
            );
            sleep(Duration::from_millis(1500)).await;
        }
    });

    let _progress_handle = tokio::spawn(async move {
        for i in 0..=60 {
            println!("Time: {}s / 60s", i);
            sleep(Duration::from_secs(1)).await;
        }
    });

    sleep(Duration::from_secs(60)).await;

    drop(consumer_dies_handle);
}
