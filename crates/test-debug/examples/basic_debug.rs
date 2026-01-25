fn main() {
    let x = 42;
    let y = hotpath::dbg!(x * 2);
    let name = "test";
    hotpath::dbg!(name);
    let _ = hotpath::dbg!(y + 1);

    std::thread::sleep(std::time::Duration::from_millis(
        std::env::var("TEST_SLEEP_MS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(0),
    ));

    println!("Hello, world!");
}
