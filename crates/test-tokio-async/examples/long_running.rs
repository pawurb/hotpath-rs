use futures_util::stream::{self, StreamExt};
use rand::Rng;
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tokio::time::sleep;

#[cfg_attr(feature = "hotpath", hotpath::measure)]
fn fast_sync_allocator() -> Vec<Vec<u64>> {
    let mut rng = rand::thread_rng();
    let num_arrays = rng.gen_range(1..=10);
    let mut arrays = Vec::new();

    for _ in 0..num_arrays {
        let size = rng.gen_range(10..100);
        let data: Vec<u64> = (0..size).map(|_| rng.gen()).collect();
        std::hint::black_box(&data);
        arrays.push(data);
    }

    std::thread::sleep(Duration::from_micros(rng.gen_range(10..50)));
    arrays
}

#[cfg_attr(feature = "hotpath", hotpath::measure)]
fn medium_sync_allocator() -> Vec<Vec<u64>> {
    let mut rng = rand::thread_rng();
    let num_arrays = rng.gen_range(1..=10);
    let mut arrays = Vec::new();

    for _ in 0..num_arrays {
        let size = rng.gen_range(100..1000);
        let data: Vec<u64> = (0..size).map(|_| rng.gen()).collect();
        std::hint::black_box(&data);
        arrays.push(data);
    }

    std::thread::sleep(Duration::from_micros(rng.gen_range(50..150)));
    arrays
}

#[cfg_attr(feature = "hotpath", hotpath::measure)]
fn slow_sync_allocator() -> Vec<Vec<u64>> {
    let mut rng = rand::thread_rng();
    let num_arrays = rng.gen_range(1..=10);
    let mut arrays = Vec::new();

    for _ in 0..num_arrays {
        let size = rng.gen_range(1000..10000);
        let data: Vec<u64> = (0..size).map(|_| rng.gen()).collect();
        std::hint::black_box(&data);
        arrays.push(data);
    }

    std::thread::sleep(Duration::from_micros(rng.gen_range(100..300)));
    arrays
}

#[cfg_attr(feature = "hotpath", hotpath::measure)]
async fn fast_async_allocator() -> Vec<Vec<u64>> {
    let mut rng = rand::thread_rng();
    let num_arrays = rng.gen_range(1..=10);
    let mut arrays = Vec::new();

    for _ in 0..num_arrays {
        let size = rng.gen_range(10..100);
        let data: Vec<u64> = (0..size).map(|_| rng.gen()).collect();
        std::hint::black_box(&data);
        arrays.push(data);
    }

    sleep(Duration::from_micros(rng.gen_range(10..50))).await;
    arrays
}

#[cfg_attr(feature = "hotpath", hotpath::measure)]
async fn slow_async_allocator() -> Vec<Vec<u64>> {
    let mut rng = rand::thread_rng();
    let num_arrays = rng.gen_range(1..=10);
    let mut arrays = Vec::new();

    for _ in 0..num_arrays {
        let size = rng.gen_range(1000..5000);
        let data: Vec<u64> = (0..size).map(|_| rng.gen()).collect();
        std::hint::black_box(&data);
        arrays.push(data);
    }

    sleep(Duration::from_micros(rng.gen_range(100..400))).await;
    arrays
}

/// Async function designed to migrate between threads.
/// Many yield points give the executor opportunities to reschedule on different workers.
#[cfg_attr(feature = "hotpath", hotpath::measure)]
async fn cross_thread_worker() -> u64 {
    let mut total = 0u64;

    // Many yield points to maximize chance of thread migration
    for i in 0..20 {
        tokio::task::yield_now().await;
        tokio::task::yield_now().await;
        tokio::task::yield_now().await;
        sleep(Duration::from_micros(1)).await;
        total += i;
    }

    total
}

/// Another async function with many awaits to demonstrate cross-thread behavior.
#[cfg_attr(feature = "hotpath", hotpath::measure)]
async fn heavy_async_work() -> Vec<u64> {
    let mut results = Vec::new();

    for _ in 0..10 {
        // CPU work
        let data: Vec<u64> = (0..100).map(|x| x * 2).collect();
        results.extend(data.iter().take(5));

        // Multiple yields per iteration
        tokio::task::yield_now().await;
        tokio::task::yield_now().await;
        sleep(Duration::from_micros(1)).await;
        tokio::task::yield_now().await;
    }

    results
}

#[cfg_attr(feature = "hotpath", hotpath::measure)]
fn process_data(arrays: Vec<Vec<u64>>) -> u64 {
    let mut rng = rand::thread_rng();
    let mut total_sum = 0u64;

    for data in arrays {
        let sum: u64 = data
            .iter()
            .take(rng.gen_range(5..20))
            .fold(0u64, |acc, &x| acc.wrapping_add(x % 1000));
        total_sum = total_sum.wrapping_add(sum);
    }

    std::hint::black_box(total_sum);
    total_sum
}

// ============================================================================
// Thread State Simulation Functions
// These functions demonstrate various thread states visible in the TUI
// ============================================================================

/// Thread that waits on a mutex - shows "Sleeping" state while waiting for lock
fn mutex_contention_worker(mutex: Arc<Mutex<u64>>, id: u32) {
    std::thread::Builder::new()
        .name(format!("mutex-worker-{}", id))
        .spawn(move || {
            for _ in 0..100 {
                // Acquire lock and hold it briefly
                let mut guard = mutex.lock().unwrap();
                *guard = guard.wrapping_add(1);
                // Do some work while holding the lock
                std::thread::sleep(Duration::from_millis(50));
                drop(guard);
                // Small gap before next acquisition
                std::thread::sleep(Duration::from_millis(10));
            }
        })
        .expect("Failed to spawn mutex worker thread");
}

/// Thread that parks itself - shows "Sleeping" (S on Linux, 3 on macOS) state
fn parked_thread_worker(unpark_signal: Arc<(Mutex<bool>, Condvar)>) {
    std::thread::Builder::new()
        .name("parked-thread".into())
        .spawn(move || {
            loop {
                // Park the thread - it will show as "Sleeping" state
                std::thread::park();

                // Check if we should exit
                let (lock, _) = &*unpark_signal;
                if *lock.lock().unwrap() {
                    break;
                }

                // Do a tiny bit of work then park again
                std::hint::black_box(42u64);
            }
        })
        .expect("Failed to spawn parked thread");
}

/// Thread waiting on a condvar - shows "Sleeping" state
fn condvar_waiter_worker(condvar_pair: Arc<(Mutex<bool>, Condvar)>, id: u32) {
    std::thread::Builder::new()
        .name(format!("condvar-wait-{}", id))
        .spawn(move || {
            let (lock, cvar) = &*condvar_pair;
            for _ in 0..50 {
                // Wait on condvar - thread will be in "Sleeping" state
                let mut ready = lock.lock().unwrap();
                while !*ready {
                    ready = cvar.wait(ready).unwrap();
                }
                *ready = false;

                // Do some work
                std::hint::black_box(123u64);
                std::thread::sleep(Duration::from_millis(20));
            }
        })
        .expect("Failed to spawn condvar waiter thread");
}

/// Thread that signals condvar waiters periodically
fn condvar_signaler_worker(condvar_pair: Arc<(Mutex<bool>, Condvar)>) {
    std::thread::Builder::new()
        .name("condvar-signal".into())
        .spawn(move || {
            for _ in 0..200 {
                std::thread::sleep(Duration::from_millis(100));
                let (lock, cvar) = &*condvar_pair;
                let mut ready = lock.lock().unwrap();
                *ready = true;
                cvar.notify_all();
                drop(ready);
            }
        })
        .expect("Failed to spawn condvar signaler thread");
}

/// Thread doing CPU-intensive work - shows "Running" state (R on Linux, 1 on macOS)
fn cpu_intensive_worker(stop_flag: Arc<Mutex<bool>>) {
    std::thread::Builder::new()
        .name("cpu-intensive".into())
        .spawn(move || {
            let mut counter = 0u64;
            loop {
                // Check stop flag periodically
                if counter.is_multiple_of(1_000_000) && *stop_flag.lock().unwrap() {
                    break;
                }
                // CPU-intensive work - should show as "Running"
                counter = counter.wrapping_add(1);
                std::hint::black_box(counter);
            }
        })
        .expect("Failed to spawn CPU intensive thread");
}

/// Thread doing blocking I/O - shows "Blocked" state (D on Linux, 4 on macOS)
/// Note: True "D" (uninterruptible sleep) is hard to trigger from userspace
/// This uses file I/O which may briefly show blocked state
fn blocking_io_worker(stop_flag: Arc<Mutex<bool>>) {
    std::thread::Builder::new()
        .name("blocking-io".into())
        .spawn(move || {
            let temp_dir = std::env::temp_dir();
            let file_path = temp_dir.join("hotpath_test_io.tmp");

            for i in 0u64.. {
                if *stop_flag.lock().unwrap() {
                    break;
                }

                // Write to file (may briefly show as blocked during I/O)
                let data: Vec<u8> = (0..4096).map(|x| (x % 256) as u8).collect();
                if std::fs::write(&file_path, &data).is_ok() {
                    // Sync to disk - more likely to show blocked state
                    if let Ok(file) = std::fs::File::open(&file_path) {
                        let _ = file.sync_all();
                    }
                }

                // Read back
                let _ = std::fs::read(&file_path);

                if i % 10 == 0 {
                    std::thread::sleep(Duration::from_millis(50));
                }
            }

            // Cleanup
            let _ = std::fs::remove_file(&file_path);
        })
        .expect("Failed to spawn blocking I/O thread");
}

/// Thread that alternates between running and sleeping states
fn alternating_state_worker(stop_flag: Arc<Mutex<bool>>) {
    std::thread::Builder::new()
        .name("alternating".into())
        .spawn(move || {
            let mut counter = 0u64;
            loop {
                if *stop_flag.lock().unwrap() {
                    break;
                }

                // "Running" phase - CPU work
                for _ in 0..100_000 {
                    counter = counter.wrapping_add(1);
                    std::hint::black_box(counter);
                }

                // "Sleeping" phase
                std::thread::sleep(Duration::from_millis(100));
            }
        })
        .expect("Failed to spawn alternating thread");
}

#[tokio::main]
#[cfg_attr(feature = "hotpath", hotpath::main)]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Starting 60-second profiling test...");
    println!("Spawning threads with various states for TUI demonstration...");

    // =========================================================================
    // Spawn thread state demonstration threads
    // =========================================================================

    // Shared mutex for contention demo - multiple threads will compete for this
    let contended_mutex = Arc::new(Mutex::new(0u64));
    for id in 0..3 {
        mutex_contention_worker(Arc::clone(&contended_mutex), id);
    }

    // Parked thread demo - thread parks itself and gets unparked periodically
    let parked_signal = Arc::new((Mutex::new(false), Condvar::new()));
    parked_thread_worker(Arc::clone(&parked_signal));

    // Condvar demo - threads waiting on condition variable
    let condvar_pair = Arc::new((Mutex::new(false), Condvar::new()));
    for id in 0..2 {
        condvar_waiter_worker(Arc::clone(&condvar_pair), id);
    }
    condvar_signaler_worker(Arc::clone(&condvar_pair));

    // CPU-intensive thread - should show as "Running" frequently
    let cpu_stop_flag = Arc::new(Mutex::new(false));
    cpu_intensive_worker(Arc::clone(&cpu_stop_flag));

    // Blocking I/O thread - may show "Blocked" during disk operations
    let io_stop_flag = Arc::new(Mutex::new(false));
    blocking_io_worker(Arc::clone(&io_stop_flag));

    // Alternating state thread - switches between Running and Sleeping
    let alt_stop_flag = Arc::new(Mutex::new(false));
    alternating_state_worker(Arc::clone(&alt_stop_flag));

    println!("Thread state demo threads spawned:");
    println!("  - 3x mutex-worker-N: Competing for a shared mutex (Sleeping while waiting)");
    println!("  - 1x parked-thread: Parked thread (Sleeping)");
    println!("  - 2x condvar-wait-N: Waiting on condition variable (Sleeping)");
    println!("  - 1x condvar-signal: Signaling condition variable");
    println!("  - 1x cpu-intensive: CPU-bound work (Running)");
    println!("  - 1x blocking-io: File I/O operations (may show Blocked)");
    println!("  - 1x alternating: Alternates between Running and Sleeping");
    println!();

    let (fast_tx, fast_rx) = mpsc::channel::<u64>(100);
    let (slow_tx, slow_rx) = mpsc::channel::<String>(50);

    #[cfg(feature = "hotpath")]
    let (fast_tx, fast_rx) =
        hotpath::channel!((fast_tx, fast_rx), label = "fast_metrics", log = true);
    #[cfg(feature = "hotpath")]
    let (slow_tx, slow_rx) = hotpath::channel!((slow_tx, slow_rx), label = "slow_events");

    let mut fast_rx = fast_rx;
    let mut slow_rx = slow_rx;

    let fast_stream = stream::iter(0u64..);
    let slow_stream = stream::iter(0u64..);

    #[cfg(feature = "hotpath")]
    let fast_stream = hotpath::stream!(fast_stream, label = "fast_metrics_stream", log = true);
    #[cfg(feature = "hotpath")]
    let slow_stream = hotpath::stream!(slow_stream, label = "slow_status_stream");

    // Pin the streams for consumption
    let mut fast_stream = Box::pin(fast_stream);
    let mut slow_stream = Box::pin(slow_stream);

    // Spawn fast channel consumer
    let fast_consumer = tokio::spawn(async move {
        let mut count = 0u64;
        while let Some(value) = fast_rx.recv().await {
            count = count.wrapping_add(value);
            if count.is_multiple_of(1000) {
                std::hint::black_box(count);
            }
        }
    });

    // Spawn slow channel consumer
    let slow_consumer = std::thread::spawn(move || {
        while let Some(msg) = slow_rx.blocking_recv() {
            std::hint::black_box(msg.len());
        }
    });

    let start = Instant::now();
    let duration = Duration::from_secs(60);
    let mut iteration = 0;

    while start.elapsed() < duration {
        iteration += 1;
        let elapsed = start.elapsed().as_secs();

        if iteration % 10 == 0 {
            println!(
                "[{:>2}s] Iteration {}: Calling mixed sync/async functions...",
                elapsed, iteration
            );
        }

        let mut rng = rand::thread_rng();

        // Send data to fast channel frequently
        let _ = fast_tx.send(rng.gen()).await;

        // Send data to slow channel occasionally
        if iteration % 5 == 0 {
            let _ = slow_tx
                .send(format!("Event at iteration {}", iteration))
                .await;
        }

        // Consume from fast stream frequently
        if let Some(value) = fast_stream.next().await {
            std::hint::black_box(value);
        }

        // Consume from slow stream occasionally
        if iteration % 7 == 0 {
            if let Some(value) = slow_stream.next().await {
                std::hint::black_box(value);
            }
        }

        // Call allocator functions which now randomly allocate 1-10 arrays each
        // Run some sync functions on separate threads to show different TIDs
        let data1_task = tokio::task::spawn_blocking(fast_sync_allocator);
        let data2_task = tokio::task::spawn_blocking(medium_sync_allocator);

        if iteration % 3 == 0 {
            let data3_task = tokio::task::spawn_blocking(|| {
                let data = slow_sync_allocator();
                process_data(data)
            });
            let _ = data3_task.await;
        }

        let data4 = fast_async_allocator().await;
        let data4_task = tokio::task::spawn_blocking(move || process_data(data4));
        let _ = data4_task.await;

        if iteration % 2 == 0 {
            let data5 = slow_async_allocator().await;
            let data5_task = tokio::task::spawn_blocking(move || process_data(data5));
            let _ = data5_task.await;
        }

        // Call cross-thread async functions (may migrate between worker threads)
        // Spawn them as separate tasks to increase migration likelihood
        let cross1 = tokio::spawn(cross_thread_worker());
        let cross2 = tokio::spawn(cross_thread_worker());
        let cross3 = tokio::spawn(heavy_async_work());

        // Also call directly (will run on current worker but may migrate)
        let _ = cross_thread_worker().await;
        let _ = heavy_async_work().await;

        let _ = cross1.await;
        let _ = cross2.await;
        let _ = cross3.await;

        let data1 = data1_task.await.unwrap();
        let data1_process_task = tokio::task::spawn_blocking(move || process_data(data1));
        let _ = data1_process_task.await;

        if iteration % 4 == 0 {
            let data2 = data2_task.await.unwrap();
            let data2_process_task = tokio::task::spawn_blocking(move || process_data(data2));
            let _ = data2_process_task.await;
        } else {
            // Still need to consume data2_task to avoid leaking it
            let _ = data2_task.await;
        }

        sleep(Duration::from_millis(rng.gen_range(10..50))).await;

        #[cfg(feature = "hotpath")]
        hotpath::measure_block!("iteration_block", {
            let temp: Vec<u32> = (0..rng.gen_range(50..200)).map(|_| rng.gen()).collect();
            std::hint::black_box(&temp);
        });
    }

    // Close channels
    drop(fast_tx);
    drop(slow_tx);

    // Wait for consumers to finish
    let _ = fast_consumer.await;

    slow_consumer.join().unwrap();

    // Signal demo threads to stop
    println!("Signaling demo threads to stop...");
    *cpu_stop_flag.lock().unwrap() = true;
    *io_stop_flag.lock().unwrap() = true;
    *alt_stop_flag.lock().unwrap() = true;

    // Signal parked thread to exit
    {
        let (lock, _) = &*parked_signal;
        *lock.lock().unwrap() = true;
    }
    // Note: The parked thread and other demo threads will eventually exit
    // We don't join them here to avoid blocking the main thread

    // Give threads a moment to clean up
    std::thread::sleep(Duration::from_millis(100));

    Ok(())
}
