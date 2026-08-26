//! Run with:
//!   cargo run -p test-all-features --example basic_all_features --all-features

use std::sync::Arc;
use std::time::Duration;

#[hotpath::measure]
fn sync_function(sleep: u64) {
    let vec1 = vec![
        1, 2, 3, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20,
    ];
    std::hint::black_box(&vec1);
    drop(vec1);
    let vec2 = vec![
        1, 2, 3, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20,
    ];
    std::hint::black_box(&vec2);
    std::thread::sleep(Duration::from_nanos(sleep));
}

#[hotpath::measure]
async fn async_function(sleep: u64) {
    let vec1 = vec![1, 2, 3, 5, 6, 7, 8, 9, 10];
    std::hint::black_box(&vec1);
    drop(vec1);
    let vec = vec![1, 2, 3, 5, 6, 7, 8, 9, 10];
    std::hint::black_box(&vec);
    tokio::time::sleep(Duration::from_nanos(sleep)).await;
}

#[hotpath::measure_all]
mod measured_module {
    #[allow(unused)]
    pub fn sync_function() {}
}

#[tokio::main(flavor = "current_thread")]
#[hotpath::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (tx, mut rx) =
        hotpath::channel!(tokio::sync::mpsc::channel::<u64>(10), label = "work-queue");

    let mutex = Arc::new(hotpath::mutex!(
        tokio::sync::Mutex::new(0u64),
        label = "counter"
    ));
    let rw_lock = Arc::new(hotpath::rw_lock!(
        tokio::sync::RwLock::new(0u64),
        label = "state"
    ));

    let producer = tokio::spawn({
        let mutex = Arc::clone(&mutex);
        let rw_lock = Arc::clone(&rw_lock);
        async move {
            for i in 0..100u64 {
                tx.send(i).await.expect("receiver dropped");
                {
                    let mut v = mutex.lock().await;
                    *v += 1;
                    tokio::time::sleep(Duration::from_nanos(i * 10)).await;
                }
                let r = rw_lock.read().await;
                std::hint::black_box(*r);
            }
        }
    });

    for i in 0..100 {
        sync_function(i);
        async_function(i * 2).await;
        hotpath::measure_block!("custom_block", {
            if i == 0 {
                println!("i ran");
            }
            std::thread::sleep(Duration::from_nanos(i * 3))
        });

        let msg = rx.recv().await.expect("sender dropped");
        {
            let mut v = mutex.lock().await;
            *v += msg;
            tokio::time::sleep(Duration::from_nanos(i * 5)).await;
        }

        if i % 10 == 0 {
            let mut w = rw_lock.write().await;
            *w += 1;
        } else {
            let r = rw_lock.read().await;
            std::hint::black_box(*r);
        }
    }

    producer.await?;

    Ok(())
}
