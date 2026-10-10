//! Attributes placed below `#[hotpath::main]` must survive its expansion.
//!
//! Run with:
//!   cargo run -p test-tokio-async --example main_attrs_below --features hotpath

use std::time::Duration;

#[hotpath::measure]
async fn example_function() {
    tokio::time::sleep(Duration::from_millis(1)).await;
}

#[hotpath::main]
#[tokio::main(flavor = "current_thread")]
async fn main() {
    for _ in 0..5 {
        example_function().await;
    }
}
