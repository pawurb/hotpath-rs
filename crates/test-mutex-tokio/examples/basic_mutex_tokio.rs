use std::sync::Arc;
use std::time::Duration;

#[tokio::main]
async fn main() {
    let _guard = hotpath::HotpathGuardBuilder::new("main")
        .sections(vec![hotpath::Section::Mutexes])
        .build();

    let lock = Arc::new(hotpath::mutex!(
        tokio::sync::Mutex::new(0u64),
        label = "counter"
    ));

    let mut handles = Vec::new();
    for _ in 0..5 {
        let lock = Arc::clone(&lock);
        handles.push(tokio::spawn(async move {
            let mut v = lock.lock().await;
            *v += 1;
            tokio::task::yield_now().await;
        }));
    }

    for h in handles {
        h.await.unwrap();
    }

    println!("Final value: {}", *lock.lock().await);
    println!("tokio Mutex example completed!");

    if let Ok(secs) = std::env::var("TEST_SLEEP_SECONDS") {
        if let Ok(duration) = secs.parse::<u64>() {
            std::thread::sleep(Duration::from_secs(duration));
        }
    }
}
