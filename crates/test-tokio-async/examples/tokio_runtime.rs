#[hotpath::measure]
fn do_work() {
    let data: Vec<u64> = (0..1000).map(|x| x * 2).collect();
    std::hint::black_box(&data);
}

#[tokio::main]
#[hotpath::main]
async fn main() {
    hotpath::tokio_runtime!();

    for _ in 0..10 {
        do_work();
        tokio::task::yield_now().await;
    }

    std::thread::sleep(std::time::Duration::from_millis(
        std::env::var("TEST_SLEEP_MS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(0),
    ));
}
