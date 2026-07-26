//! Run with:
//!   cargo run -p test-all-features --example basic_all_features --all-features

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
    for i in 0..100 {
        sync_function(i);
        async_function(i * 2).await;
        hotpath::measure_block!("custom_block", {
            if i == 0 {
                println!("i ran");
            }
            std::thread::sleep(Duration::from_nanos(i * 3))
        });
    }

    Ok(())
}
