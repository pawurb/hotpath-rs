use std::sync::Arc;
use std::thread;
use std::time::Duration;

fn main() {
    let _guard = hotpath::HotpathGuardBuilder::new("main")
        .sections(vec![hotpath::Section::Mutexes])
        .build();

    let lock = Arc::new(hotpath::mutex!(
        std::sync::Mutex::new(0u64),
        label = "counter"
    ));

    let mut handles = Vec::new();
    for _ in 0..5 {
        let lock = Arc::clone(&lock);
        handles.push(thread::spawn(move || {
            let mut v = lock.lock().unwrap();
            *v += 1;
            thread::sleep(Duration::from_millis(10));
        }));
    }

    for h in handles {
        h.join().unwrap();
    }

    println!("Final value: {}", *lock.lock().unwrap());
    println!("Std Mutex example completed!");

    if let Ok(secs) = std::env::var("TEST_SLEEP_SECONDS") {
        if let Ok(duration) = secs.parse::<u64>() {
            thread::sleep(Duration::from_secs(duration));
        }
    }
}
