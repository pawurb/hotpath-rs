use std::time::Duration;
use tokio::sync::mpsc;

// Validate that macros and noop methods work without hotpath feature.
#[tokio::main(flavor = "current_thread")]
#[hotpath::main(
    percentiles = [50, 90, 95, 99],
    format = "json",
    limit = 10,
    output_path = "/dev/null",
    report = "functions-timing,channels,streams,futures,threads"
)]
async fn main() {
    sync_no_params();
    let _v = sync_with_log();
    async_with_future().await;
    let _v = async_with_log_and_future().await;
    future_fn_no_params().await;
    let _v = future_fn_with_log().await;

    let block_result = hotpath::measure_block!("noop_block", 1 + 2);
    assert_eq!(block_result, 3);

    let dbg_val = hotpath::dbg!(42);
    assert_eq!(dbg_val, 42);
    let (a, b, c) = hotpath::dbg!(1, 2, 3);
    assert_eq!((a, b, c), (1, 2, 3));

    let state = "running";
    hotpath::val!("state").set(&state);

    hotpath::gauge!("counter").set(0.0).inc(5.0).dec(2.0);
    hotpath::gauge!("int_gauge").set(42_i64);
    hotpath::gauge!("uint_gauge").inc(1_u32);
    hotpath::gauge!("float_gauge").dec(0.5_f32);

    let (_tx, _rx) = hotpath::channel!(mpsc::channel::<String>(10));
    let (_tx, _rx) = hotpath::channel!(mpsc::channel::<String>(10), label = "labeled");
    let (_tx, _rx) = hotpath::channel!(mpsc::channel::<String>(10), log = true);
    let (_tx, _rx) = hotpath::channel!(mpsc::channel::<String>(10), label = "both", log = true);
    let (_tx, _rx) = hotpath::channel!(mpsc::channel::<String>(10), log = true, label = "rev");

    let _s = hotpath::stream!(futures::stream::iter(1..=3));
    let _s = hotpath::stream!(futures::stream::iter(1..=3), label = "labeled_stream");
    let _s = hotpath::stream!(futures::stream::iter(1..=3), log = true);
    let _s = hotpath::stream!(
        futures::stream::iter(1..=3),
        label = "both_stream",
        log = true
    );
    let _s = hotpath::stream!(
        futures::stream::iter(1..=3),
        log = true,
        label = "rev_stream"
    );

    let _v = hotpath::future!(async { 1 }).await;
    let _v = hotpath::future!(async { 2 }, label = "labeled_future").await;
    let _v = hotpath::future!(async { 3 }, log = true).await;
    let _v = hotpath::future!(async { 4 }, label = "both_future", log = true).await;
    let _v = hotpath::future!(async { 5 }, log = true, label = "rev_future").await;

    hotpath::tokio_runtime!();
    let _handle = tokio::runtime::Handle::current();
    hotpath::tokio_runtime!(&_handle);

    measured_mod::mod_fn();

    let calc = Calculator::new(10);
    let _sum = calc.add(5);
    let _skipped = calc.skipped_method();

    let _guard = hotpath::HotpathGuardBuilder::new("noop_builder")
        .percentiles(&[50, 95, 99])
        .format(hotpath::Format::Table)
        .with_functions_limit(20)
        .with_channels_limit(10)
        .with_streams_limit(10)
        .with_futures_limit(10)
        .with_threads_limit(5)
        .output_path("/dev/null")
        .with_sections(vec![
            hotpath::Section::FunctionsTiming,
            hotpath::Section::Channels,
        ])
        .before_shutdown(|| {})
        .build();

    println!("all_noop_ok");
}

#[hotpath::measure]
fn sync_no_params() {
    std::thread::sleep(Duration::from_nanos(1));
}

#[hotpath::measure(log = true)]
fn sync_with_log() -> i32 {
    42
}

#[hotpath::measure(future = true)]
async fn async_with_future() {
    tokio::time::sleep(Duration::from_nanos(1)).await;
}

#[hotpath::measure(log = true, future = true)]
async fn async_with_log_and_future() -> String {
    "result".to_string()
}

#[hotpath::future_fn]
async fn future_fn_no_params() {
    tokio::time::sleep(Duration::from_nanos(1)).await;
}

#[hotpath::future_fn(log = true)]
async fn future_fn_with_log() -> Vec<u8> {
    vec![1, 2, 3]
}

#[hotpath::measure_all]
mod measured_mod {
    pub fn mod_fn() {}

    #[hotpath::skip]
    #[allow(dead_code)]
    pub fn skipped_fn() {}
}

struct Calculator {
    value: i32,
}

#[hotpath::measure_all]
impl Calculator {
    fn new(value: i32) -> Self {
        Self { value }
    }

    fn add(&self, n: i32) -> i32 {
        self.value + n
    }

    #[hotpath::skip]
    fn skipped_method(&self) -> i32 {
        self.value
    }
}
