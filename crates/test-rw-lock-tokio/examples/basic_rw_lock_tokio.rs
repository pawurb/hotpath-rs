use std::sync::Arc;
use std::time::Duration;

#[tokio::main]
async fn main() {
    let _guard = hotpath::HotpathGuardBuilder::new("main")
        .sections(vec![hotpath::Section::RwLocks])
        .build();

    // wrap-prefix drop-in smoke test (instrumented build)
    #[cfg(feature = "hotpath")]
    {
        #[allow(deprecated)]
        let wrapped = hotpath::wrap::tokio::sync::RwLock::new(0u64);
        let _ = *wrapped.read().await;
    }

    let lock = Arc::new(hotpath::rw_lock!(
        tokio::sync::RwLock::new(0u64),
        label = "counter"
    ));

    for _ in 0..3 {
        let mut w = lock.write().await;
        *w += 1;
        tokio::task::yield_now().await;
    }

    for _ in 0..5 {
        let r = lock.read().await;
        let _value = *r;
        tokio::task::yield_now().await;
    }

    println!("Final value: {}", *lock.read().await);

    println!("tokio RwLock example completed!");

    if let Ok(secs) = std::env::var("TEST_SLEEP_SECONDS") {
        if let Ok(duration) = secs.parse::<u64>() {
            std::thread::sleep(Duration::from_secs(duration));
        }
    }
}
