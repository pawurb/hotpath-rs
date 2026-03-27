#[hotpath::main]
fn main() {
    // gauge! macros
    hotpath::gauge!("queue_size").set(10).inc(5).dec(3);
    hotpath::gauge!("connections").set(42);

    // val! macros
    let counter = 99;
    hotpath::val!("counter").set(&counter);
    hotpath::val!("status").set(&"running");

    // dbg! macros
    hotpath::dbg!(7 * 2);
    hotpath::dbg!("hello");

    // Give the background thread time to process events
    std::thread::sleep(std::time::Duration::from_millis(
        std::env::var("TEST_SLEEP_MS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(50),
    ));
}
