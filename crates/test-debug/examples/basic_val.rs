//! Run with:
//!   cargo run -p test-debug --example basic_val --features hotpath

fn main() {
    let counter = 42;
    hotpath::val!("counter").set(&counter);
    hotpath::val!("counter").set(&(counter + 1));
    let dynamic_key = format!("status_{}", 1);
    hotpath::val!(dynamic_key).set(&"running");

    std::thread::sleep(std::time::Duration::from_millis(
        std::env::var("TEST_SLEEP_MS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(0),
    ));

    println!("Hello, val!");
}
