use futures_util::stream::{self, StreamExt};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

pub fn init() {
    spawn_tokio_demo();
    spawn_rw_locks();
    spawn_mutexes();
    spawn_channels();
    spawn_std_channel();
}

fn spawn_channels() {
    // Wrap channel: tracks exact send->receive latency. The consumer outpaces the
    // producer, so the bounded channel stays near empty and each message is processed
    // as soon as it arrives - a healthy channel that keeps up with its load.
    let (tx, rx) = hotpath::channel!(
        crossbeam_channel::bounded::<u64>(8),
        wrap = true,
        label = "demo-jobs"
    );

    thread::spawn(move || {
        let mut i = 0u64;
        while tx.send(i).is_ok() {
            i += 1;
            thread::sleep(Duration::from_millis(40));
        }
    });

    thread::spawn(move || {
        while let Ok(job) = rx.recv() {
            std::hint::black_box(job);
            thread::sleep(Duration::from_millis(15));
        }
    });
}

fn spawn_std_channel() {
    // Endpoint-wrapped std::sync::mpsc channel. Bounded wrappers need an explicit
    // `capacity` (std doesn't expose it). The producer outpaces the consumer so the
    // self-tracked queue depth climbs to the bound.
    let (tx, rx) = hotpath::channel!(
        std::sync::mpsc::sync_channel::<u64>(8),
        wrap = true,
        capacity = 8,
        label = "demo-std-jobs"
    );

    thread::spawn(move || {
        let mut i = 0u64;
        while tx.send(i).is_ok() {
            i += 1;
            thread::sleep(Duration::from_millis(10));
        }
    });

    thread::spawn(move || {
        while let Ok(job) = rx.recv() {
            std::hint::black_box(job);
            thread::sleep(Duration::from_millis(30));
        }
    });
}

fn spawn_mutexes() {
    let lock = Arc::new(hotpath::mutex!(
        std::sync::Mutex::new(0u64),
        label = "demo-mutex"
    ));

    // A few contending threads holding the lock for varying durations.
    for delay_ms in [60u64, 90, 130] {
        let lock = Arc::clone(&lock);
        thread::spawn(move || loop {
            {
                let mut v = lock.lock().unwrap();
                *v += 1;
                thread::sleep(Duration::from_millis(8));
            }
            thread::sleep(Duration::from_millis(delay_ms));
        });
    }
}

fn spawn_rw_locks() {
    let lock = Arc::new(hotpath::rw_lock!(
        std::sync::RwLock::new(0u64),
        label = "demo-counter"
    ));

    // Writer: bumps the counter periodically, holding the write lock briefly.
    let writer = Arc::clone(&lock);
    thread::spawn(move || loop {
        {
            let mut w = writer.write().unwrap();
            *w += 1;
            thread::sleep(Duration::from_millis(5));
        }
        thread::sleep(Duration::from_millis(120));
    });

    // Readers: a few threads sampling the counter with varying hold times.
    for delay_ms in [40u64, 70, 110] {
        let reader = Arc::clone(&lock);
        thread::spawn(move || loop {
            {
                let r = reader.read().unwrap();
                std::hint::black_box(*r);
                thread::sleep(Duration::from_millis(2));
            }
            thread::sleep(Duration::from_millis(delay_ms));
        });
    }

    // Second lock: write-heavy with longer holds than the counter.
    let config = Arc::new(hotpath::rw_lock!(
        std::sync::RwLock::new(0u64),
        label = "demo-config"
    ));

    let cfg_writer = Arc::clone(&config);
    thread::spawn(move || loop {
        {
            let mut w = cfg_writer.write().unwrap();
            *w += 1;
            thread::sleep(Duration::from_millis(15));
        }
        thread::sleep(Duration::from_millis(50));
    });

    let cfg_reader = Arc::clone(&config);
    thread::spawn(move || loop {
        {
            let r = cfg_reader.read().unwrap();
            std::hint::black_box(*r);
            thread::sleep(Duration::from_millis(1));
        }
        thread::sleep(Duration::from_millis(200));
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
            hotpath::val!("stream_number").set(&value);
            hotpath::gauge!("stream_value").set(value);
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
