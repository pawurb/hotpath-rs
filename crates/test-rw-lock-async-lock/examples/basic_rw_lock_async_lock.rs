use futures_lite::future;
use std::sync::Arc;
use std::time::Duration;

fn main() {
    let _guard = hotpath::HotpathGuardBuilder::new("main")
        .sections(vec![hotpath::Section::RwLocks])
        .build();

    future::block_on(async {
        let lock = Arc::new(hotpath::rw_lock!(
            async_lock::RwLock::new(0u64),
            label = "counter"
        ));

        for _ in 0..3 {
            let mut w = lock.write().await;
            *w += 1;
            future::yield_now().await;
        }

        for _ in 0..5 {
            let r = lock.read().await;
            let _value = *r;
            future::yield_now().await;
        }

        println!("Final value: {}", *lock.read().await);
    });

    println!("async-lock RwLock example completed!");

    if let Ok(secs) = std::env::var("TEST_SLEEP_SECONDS") {
        if let Ok(duration) = secs.parse::<u64>() {
            std::thread::sleep(Duration::from_secs(duration));
        }
    }
}
