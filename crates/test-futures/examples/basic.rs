//! Basic example demonstrating the `future!` macro for instrumenting futures.
//!
//! Run with: cargo run -p test-futures --example basic --features hotpath

use hotpath::future;
use std::time::Duration;

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

#[tokio::main]
async fn main() {
    println!("=== Basic Future Instrumentation Demo ===\n");

    // Instrument a simple async block
    println!("--- Simple async block ---");
    let result = future!(async { 1 + 1 }).await;
    println!("Result: {}\n", result);

    // Instrument an async function call
    println!("--- Async function (slow_operation) ---");
    let result = future!(slow_operation()).await;
    println!("Result: {}\n", result);

    // Instrument a multi-step async operation
    println!("--- Multi-step async operation ---");
    let result = future!(multi_step_operation()).await;
    println!("Result: {}\n", result);

    // Nested instrumented futures
    println!("--- Nested instrumented futures ---");
    let outer = future!(async {
        let inner_result = future!(async {
            tokio::time::sleep(Duration::from_millis(5)).await;
            100
        })
        .await;
        inner_result * 2
    })
    .await;
    println!("Result: {}\n", outer);

    println!("=== Demo Complete ===");
}
