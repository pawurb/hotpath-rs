use std::sync::Arc;
use std::thread;
use std::time::Duration;

fn main() {
    let _guard = hotpath::HotpathGuardBuilder::new("main")
        .sections(vec![hotpath::Section::RwLocks])
        .build();

    // wrap-prefix drop-in smoke test (instrumented build)
    #[cfg(feature = "hotpath")]
    {
        #[allow(deprecated)]
        let wrapped = hotpath::wrap::std::sync::RwLock::new(0u64);
        let _ = *wrapped.read().unwrap();
    }

    let lock = Arc::new(hotpath::rw_lock!(
        std::sync::RwLock::new(0u64),
        label = "counter"
    ));

    let mut handles = Vec::new();
    for _ in 0..3 {
        let lock = Arc::clone(&lock);
        handles.push(thread::spawn(move || {
            let mut w = lock.write().unwrap();
            *w += 1;
            thread::sleep(Duration::from_millis(10));
        }));
    }

    for h in handles {
        h.join().unwrap();
    }

    let mut handles = Vec::new();
    for _ in 0..5 {
        let lock = Arc::clone(&lock);
        handles.push(thread::spawn(move || {
            let r = lock.read().unwrap();
            let _value = *r;
            thread::sleep(Duration::from_millis(5));
        }));
    }

    for h in handles {
        h.join().unwrap();
    }

    println!("Final value: {}", *lock.read().unwrap());
    println!("Std RwLock example completed!");

    if let Ok(secs) = std::env::var("TEST_SLEEP_SECONDS") {
        if let Ok(duration) = secs.parse::<u64>() {
            thread::sleep(Duration::from_secs(duration));
        }
    }
}
