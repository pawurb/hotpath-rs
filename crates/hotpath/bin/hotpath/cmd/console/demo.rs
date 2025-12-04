use futures_util::stream::{self, StreamExt};
use std::thread;
use std::time::Duration;

pub fn init() {
    spawn_bounded_channel();
    spawn_unbounded_channel();
    spawn_tokio_demo();
}

fn spawn_bounded_channel() {
    let (tx, rx) = std::sync::mpsc::sync_channel::<String>(10);
    #[cfg(feature = "hotpath")]
    let (tx, rx) = hotpath::channel!((tx, rx), label = "demo-bounded", capacity = 10, log = true);

    thread::spawn(move || {
        let mut counter = 0u64;
        loop {
            let msg = format!("Message {}", counter);
            if tx.send(msg).is_err() {
                break;
            }
            counter += 1;
            thread::sleep(Duration::from_millis(100));
        }
    });

    thread::spawn(move || {
        while let Ok(_msg) = rx.recv() {
            thread::sleep(Duration::from_millis(150));
        }
    });
}

fn spawn_unbounded_channel() {
    let (tx, rx) = std::sync::mpsc::channel::<u64>();
    #[cfg(feature = "hotpath")]
    let (tx, rx) = hotpath::channel!((tx, rx), label = "demo-unbounded", log = true);

    thread::spawn(move || {
        let mut counter = 0u64;
        loop {
            if tx.send(counter).is_err() {
                break;
            }
            counter += 1;
            thread::sleep(Duration::from_millis(50));
        }
    });

    thread::spawn(move || {
        while let Ok(_value) = rx.recv() {
            thread::sleep(Duration::from_millis(80));
        }
    });
}

async fn sleep_ms(ms: u64) {
    let _ = tokio::task::spawn_blocking(move || {
        thread::sleep(Duration::from_millis(ms));
    })
    .await;
}

fn spawn_tokio_demo() {
    thread::spawn(|| {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("Failed to create tokio runtime");

        rt.block_on(async {
            spawn_streams().await;
            spawn_futures_demo().await;
            std::future::pending::<()>().await;
        });
    });
}

async fn spawn_streams() {
    // Fast number stream
    let stream1 = hotpath::stream!(
        stream::iter(0u64..).then(|i| async move {
            sleep_ms(80).await;
            i
        }),
        label = "demo-number-stream",
        log = true
    );

    tokio::spawn(async move {
        let mut stream = Box::pin(stream1);
        while let Some(value) = stream.next().await {
            std::hint::black_box(value);
        }
    });

    // Text stream with slower consumption
    let texts = vec!["hello", "world", "from", "demo", "streams"];
    let stream2 = hotpath::stream!(
        stream::iter(texts.into_iter().cycle()).then(|s| async move {
            sleep_ms(200).await;
            s
        }),
        label = "demo-text-stream",
        log = true
    );

    tokio::spawn(async move {
        let mut stream = Box::pin(stream2);
        while let Some(text) = stream.next().await {
            std::hint::black_box(text);
        }
    });

    // Repeat stream
    let stream3 = hotpath::stream!(
        stream::repeat(42u64).then(|v| async move {
            sleep_ms(150).await;
            v
        }),
        label = "demo-repeat-stream"
    );

    tokio::spawn(async move {
        let mut stream = Box::pin(stream3);
        while let Some(value) = stream.next().await {
            std::hint::black_box(value);
        }
    });
}

async fn spawn_futures_demo() {
    // Spawn multiple futures that run concurrently
    tokio::spawn(async {
        loop {
            let result = hotpath::future!(
                async {
                    sleep_ms(100).await;
                    42u64
                },
                log = true
            )
            .await;
            std::hint::black_box(result);
            sleep_ms(50).await;
        }
    });

    tokio::spawn(async {
        loop {
            let result = hotpath::future!(
                async {
                    let mut sum = 0u64;
                    for i in 0..5 {
                        sleep_ms(50).await;
                        sum += i;
                    }
                    sum
                },
                log = true
            )
            .await;
            std::hint::black_box(result);
            sleep_ms(100).await;
        }
    });

    tokio::spawn(async {
        loop {
            let result = hotpath::future!(async {
                tokio::task::yield_now().await;
                tokio::task::yield_now().await;
                sleep_ms(30).await;
                "yielded"
            })
            .await;
            std::hint::black_box(result);
            sleep_ms(70).await;
        }
    });
}
