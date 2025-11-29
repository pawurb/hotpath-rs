//! Example demonstrating the `future!` macro and `#[future_fn]` attribute.
//!
//! Run with: cargo run -p test-tasks --example basic_tasks --features hotpath

use hotpath::future;
use hotpath::tasks::FuturesGuard;
use std::time::Duration;

#[allow(dead_code)]
struct NoDebug(i32);

async fn returns_no_debug() -> NoDebug {
    NoDebug(42)
}

async fn slow_operation() -> i32 {
    tokio::time::sleep(Duration::from_millis(10)).await;
    42
}

async fn multi_step_operation() -> String {
    tokio::time::sleep(Duration::from_millis(5)).await;
    let step1 = "Hello".to_string();
    tokio::time::sleep(Duration::from_millis(5)).await;
    let step2 = " World";
    step1 + step2
}

// =========================================================================
// Functions instrumented with #[future_fn] attribute macro
// =========================================================================

#[cfg_attr(feature = "hotpath", hotpath::future_fn)]
async fn attributed_no_log() -> i32 {
    tokio::time::sleep(Duration::from_millis(5)).await;
    100
}

#[cfg_attr(feature = "hotpath", hotpath::future_fn(log = true))]
async fn attributed_with_log() -> String {
    tokio::time::sleep(Duration::from_millis(5)).await;
    "attributed result".to_string()
}

#[tokio::main]
async fn main() {
    let _guard = FuturesGuard::new();

    println!("=== Future Instrumentation Demo ===\n");

    // =========================================================================
    // WITHOUT log = true (no Debug requirement)
    // =========================================================================
    println!("--- Without log = true (works with any type) ---\n");

    // Works with non-Debug types!
    println!("Future returning NoDebug type:");
    let _result = future!(returns_no_debug()).await;
    println!();

    // Also works with Debug types, just doesn't print the value
    println!("Future returning i32 (no value printed):");
    let result = future!(slow_operation()).await;
    println!("Result: {}\n", result);

    println!("--- With log = true (prints Debug output) ---\n");

    // Prints the value when Ready
    println!("Future returning i32 (value printed):");
    let result = future!(slow_operation(), log = true).await;
    println!("Result: {}\n", result);

    // Multi-step operation with logging
    println!("Multi-step future with logging:");
    let result = future!(multi_step_operation(), log = true).await;
    println!("Result: {}\n", result);

    // Nested futures with logging
    println!("Nested futures with logging:");
    let outer = future!(
        async {
            let inner_result = future!(
                async {
                    tokio::time::sleep(Duration::from_millis(5)).await;
                    100
                },
                log = true
            )
            .await;
            inner_result * 2
        },
        log = true
    )
    .await;
    println!("Result: {}\n", outer);

    // Cancelled future (never resolved)
    println!("Creating a future that will be cancelled:");
    {
        let _cancelled = future!(
            async {
                tokio::time::sleep(Duration::from_secs(1000)).await;
                "never reached"
            },
            log = true
        );
        // _cancelled is dropped here without being awaited
    }
    println!("Future was dropped without being awaited\n");

    // =========================================================================
    // Using #[future_fn] attribute macro
    // =========================================================================
    println!("--- Using #[future_fn] attribute macro ---\n");

    let _result = attributed_no_log().await;
    let _result = attributed_with_log().await;
    let _result = attributed_no_log().await;
    let _result = attributed_with_log().await;

    println!("=== Demo Complete ===\n");

    // Small delay to let background thread process all events
    tokio::time::sleep(Duration::from_millis(10)).await;

    // For testing: allow configurable sleep to keep server running
    if let Ok(secs) = std::env::var("TEST_SLEEP_SECONDS") {
        if let Ok(secs) = secs.parse::<u64>() {
            tokio::time::sleep(Duration::from_secs(secs)).await;
        }
    }
}
