use std::sync::mpsc;
use std::thread;

// Reproduces cross-thread lifecycle ordering: the worker thread's event queue
// is registered (and therefore drained) ahead of the main thread's queue that
// carries the target mutex's `Created` event, so all 100 `Released` events
// reach the worker before the entry they belong to exists.
fn main() {
    let _guard = hotpath::HotpathGuardBuilder::new("main")
        .sections(vec![hotpath::Section::Mutexes])
        .build();

    let (ready_tx, ready_rx) = mpsc::channel();
    let (target_tx, target_rx) = mpsc::channel::<hotpath::wrap::std::sync::Mutex<u64>>();

    let handle = thread::spawn(move || {
        let warmup = hotpath::mutex!(std::sync::Mutex::new(0u64), label = "warmup");
        *warmup.lock().unwrap() += 1;
        ready_tx.send(()).unwrap();

        let target = target_rx.recv().unwrap();
        for _ in 0..100 {
            *target.lock().unwrap() += 1;
        }
    });

    ready_rx.recv().unwrap();
    let target = hotpath::mutex!(std::sync::Mutex::new(0u64), label = "target");
    target_tx.send(target).unwrap();
    handle.join().unwrap();

    println!("Created ordering example completed!");
}
