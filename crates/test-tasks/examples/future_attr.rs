//! Example demonstrating the `#[hotpath::future_fn]` attribute macro.
//!
//! This shows how to use the attribute macro instead of the `future!` declarative macro.
//!
//! Run with: cargo run -p test-tasks --example future_attr --features hotpath

use hotpath::tasks::FuturesGuard;
use std::time::Duration;

// Without log = true (works with any return type)
#[cfg_attr(feature = "hotpath", hotpath::future_fn)]
async fn fetch_data() -> Vec<u8> {
    tokio::time::sleep(Duration::from_millis(10)).await;
    vec![1, 2, 3, 4, 5]
}

// With log = true (requires Debug on return type)
#[cfg_attr(feature = "hotpath", hotpath::future_fn(log = true))]
async fn compute_value() -> i32 {
    tokio::time::sleep(Duration::from_millis(10)).await;
    42
}

// Another logged function
#[cfg_attr(feature = "hotpath", hotpath::future_fn(log = true))]
async fn build_message() -> String {
    tokio::time::sleep(Duration::from_millis(5)).await;
    "Hello, World!".to_string()
}

// Function that calls other instrumented functions
#[cfg_attr(feature = "hotpath", hotpath::future_fn(log = true))]
async fn orchestrate() -> i32 {
    let data = fetch_data().await;
    let value = compute_value().await;
    let _msg = build_message().await;
    value + data.len() as i32
}

#[tokio::main]
async fn main() {
    let _guard = FuturesGuard::new();

    println!("=== #[hotpath::future] Attribute Demo ===\n");

    println!("Calling fetch_data()...");
    let data = fetch_data().await;
    println!("Got {} bytes\n", data.len());

    println!("Calling compute_value()...");
    let value = compute_value().await;
    println!("Got value: {}\n", value);

    println!("Calling build_message()...");
    let msg = build_message().await;
    println!("Got message: {}\n", msg);

    println!("Calling orchestrate()...");
    let result = orchestrate().await;
    println!("Orchestration result: {}\n", result);

    println!("=== Demo Complete ===");

    tokio::time::sleep(Duration::from_millis(10)).await;
}
