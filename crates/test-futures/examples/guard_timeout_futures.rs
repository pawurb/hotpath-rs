use std::time::Duration;

#[hotpath::future_fn]
async fn timeout_worker() -> u64 {
    tokio::task::yield_now().await;
    42
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    hotpath::futures::FuturesGuardBuilder::new().build_with_timeout(Duration::from_secs(1));

    loop {
        let _ = timeout_worker().await;
    }
}
