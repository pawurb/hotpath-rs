fn main() {
    hotpath::gauge!("queue_size").set(10.0);
    hotpath::gauge!("queue_size").inc(5.0);
    hotpath::gauge!("queue_size").dec(3);

    let dynamic_key = format!("connections_{}", 1);
    hotpath::gauge!(dynamic_key).set(42);

    std::thread::sleep(std::time::Duration::from_millis(
        std::env::var("TEST_SLEEP_MS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(0),
    ));

    println!("Hello, gauge!");
}
