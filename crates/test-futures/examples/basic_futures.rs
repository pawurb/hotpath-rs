//! Example demonstrating the `future!` macro and `#[future_fn]` attribute.
//!
//! Run with: cargo run -p test-futures --example basic_futures --features hotpath

use hotpath::future;
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

#[hotpath::future_fn]
async fn attributed_no_log() -> i32 {
    tokio::time::sleep(Duration::from_millis(5)).await;
    100
}

#[hotpath::future_fn(log = true)]
async fn attributed_with_log() -> String {
    tokio::time::sleep(Duration::from_millis(5)).await;
    "attributed result".to_string()
}

#[tokio::main]
async fn main() {
    let _guard = hotpath::HotpathGuardBuilder::new("main")
        .with_sections(vec![hotpath::Section::Futures])
        .build();

    println!("=== Futures Instrumentation Demo ===\n");

    let _result = future!(returns_no_debug()).await;

    let _result = future!(slow_operation(), label = "my_labeled_future").await;
    let _result = future!(slow_operation(), label = "labeled_with_log", log = true).await;

    println!();

    let result = future!(slow_operation()).await;
    println!("Result: {}\n", result);

    let _result = future!(slow_operation(), log = true).await;

    let _result = future!(multi_step_operation(), log = true).await;
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

    {
        let _cancelled = future!(
            async {
                tokio::time::sleep(Duration::from_secs(1000)).await;
                "never reached"
            },
            log = true
        );
    }

    let _result = attributed_no_log().await;
    let _result = attributed_with_log().await;
    let _result = attributed_no_log().await;
    let _result = attributed_with_log().await;

    tokio::time::sleep(Duration::from_millis(10)).await;

    // For testing: allow configurable sleep to keep server running
    if let Ok(secs) = std::env::var("TEST_SLEEP_SECONDS") {
        if let Ok(secs) = secs.parse::<u64>() {
            tokio::time::sleep(Duration::from_secs(secs)).await;
        }
    }
}
