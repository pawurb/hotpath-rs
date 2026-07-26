//! Run with:
//!   cargo run -p test-custom-feature --example custom_feature --features hotpath-profile

use std::time::Duration;

#[hotpath::measure]
fn allocating_function(sleep: u64) {
    let vec = vec![
        1, 2, 3, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20,
    ];
    std::hint::black_box(&vec);
    std::thread::sleep(Duration::from_nanos(sleep));
}

#[tokio::main(flavor = "current_thread")]
#[hotpath::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    for i in 0..100 {
        allocating_function(i);
    }

    if let Ok(secs) = std::env::var("TEST_SLEEP_SECONDS") {
        tokio::time::sleep(Duration::from_secs(secs.parse()?)).await;
    }

    Ok(())
}
