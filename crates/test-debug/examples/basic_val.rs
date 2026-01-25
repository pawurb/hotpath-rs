fn main() {
    let counter = 42;
    hotpath::val!("counter", counter);
    hotpath::val!("counter", counter + 1);
    hotpath::val!("status", "running");

    std::thread::sleep(std::time::Duration::from_millis(
        std::env::var("TEST_SLEEP_MS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(0),
    ));

    println!("Hello, val!");
}
