use std::thread;
use std::time::Instant;

#[hotpath::measure]
fn alloc() {
    for _ in 0..1000 {
        let vec = vec![1u8; 128];
        std::hint::black_box(vec);
    }
}

#[hotpath::main]
fn main() {
    let num_threads = std::env::var("HOTPATH_ALLOC_NUM_THREADS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(3);
    let runs_per_thread = 10_000u64;

    let start = Instant::now();
    let handles: Vec<_> = (0..num_threads)
        .map(|_| {
            thread::spawn(move || {
                for _ in 0..runs_per_thread {
                    alloc();
                }
            })
        })
        .collect();

    for handle in handles {
        handle.join().unwrap();
    }
    let elapsed = start.elapsed();

    let total = runs_per_thread * num_threads as u64;
    println!(
        "alloc: {total} calls in {elapsed:?} ({:.1} ns/op)",
        elapsed.as_nanos() as f64 / total as f64
    );
}
