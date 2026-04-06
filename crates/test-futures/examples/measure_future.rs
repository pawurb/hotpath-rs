use std::time::Duration;

#[hotpath::measure(future = true)]
async fn timed_future() -> i32 {
    tokio::time::sleep(Duration::from_millis(10)).await;
    42
}

#[hotpath::measure(future = true, log = true)]
async fn timed_future_with_log() -> String {
    tokio::time::sleep(Duration::from_millis(5)).await;
    "hello from measure+future".to_string()
}

#[hotpath::measure]
async fn timing_only() -> i32 {
    tokio::time::sleep(Duration::from_millis(5)).await;
    99
}

#[tokio::main]
async fn main() {
    let _guard = hotpath::HotpathGuardBuilder::new("main")
        .sections(vec![
            hotpath::Section::FunctionsTiming,
            hotpath::Section::Futures,
        ])
        .build();

    let _result = timed_future().await;
    let _result = timed_future().await;
    let _result = timed_future_with_log().await;
    let _result = timing_only().await;
}
