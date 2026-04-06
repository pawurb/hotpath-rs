use std::time::Duration;

#[hotpath::future_fn]
async fn timeout_worker() -> u64 {
    tokio::task::yield_now().await;
    42
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    hotpath::HotpathGuardBuilder::new("guard_timeout_futures")
        .sections(vec![hotpath::Section::Futures])
        .build_with_shutdown(Duration::from_secs(1));

    loop {
        let _ = timeout_worker().await;
    }
}
