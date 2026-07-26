//! Run with:
//!   cargo run -p test-tokio-async --example main_format --features hotpath

use std::time::Duration;

#[hotpath::measure]
fn example_function() {
    std::thread::sleep(Duration::from_millis(10));
}

#[hotpath::main(format = "json")]
fn main() {
    for _ in 0..5 {
        example_function();
    }
}
