//! Example demonstrating how the `future!` macro handles multiple futures at the same source location.
//!
//! When multiple futures are created at the same line (e.g., in a loop), the display label
//! is automatically suffixed with `-2`, `-3`, etc. to distinguish them.
//!
//! Run with: cargo run -p test-tasks --example iter_tasks --features hotpath

use hotpath::future;
use hotpath::tasks::FuturesGuard;
use std::time::Duration;

#[tokio::main]
async fn main() {
    let _guard = FuturesGuard::new();

    println!("Creating futures in loops...\n");

    // Create 3 futures at the same location
    println!("Creating 3 async futures:");
    for i in 0..3 {
        let result = future!(
            async move {
                tokio::time::sleep(Duration::from_millis(10)).await;
                i * 10
            },
            log = true
        )
        .await;
        println!("  - Future {} completed with result: {}", i, result);
    }

    // Create 3 more futures at another location
    println!("\nCreating 3 more futures:");
    for i in 0..3 {
        let result = future!(
            async move {
                tokio::time::sleep(Duration::from_millis(5)).await;
                format!("result-{}", i)
            },
            log = true
        )
        .await;
        println!("  - Future {} completed: {}", i, result);
    }

    tokio::time::sleep(Duration::from_millis(50)).await;
    println!("\nAll futures completed!");
}
