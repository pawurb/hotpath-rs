use futures_lite::future;
use std::sync::Arc;
use std::time::Duration;

fn main() {
    let _guard = hotpath::HotpathGuardBuilder::new("main")
        .sections(vec![hotpath::Section::Mutexes])
        .build();

    future::block_on(async {
        // wrap-prefix drop-in: resolves with hotpath on and off
        #[allow(deprecated)]
        let wrapped = hotpath::wrap::async_lock::Mutex::new(0u64);
        let _ = *wrapped.lock().await;

        let lock = Arc::new(hotpath::mutex!(
            async_lock::Mutex::new(0u64),
            label = "counter"
        ));

        for _ in 0..5 {
            let mut v = lock.lock().await;
            *v += 1;
            future::yield_now().await;
        }

        println!("Final value: {}", *lock.lock().await);
    });

    println!("async-lock Mutex example completed!");

    if let Ok(secs) = std::env::var("TEST_SLEEP_SECONDS") {
        if let Ok(duration) = secs.parse::<u64>() {
            std::thread::sleep(Duration::from_secs(duration));
        }
    }
}
