//! Run with:
//!   cargo run -p test-smol-async --example all_guards --features hotpath

use std::time::Duration;

async fn warm_up_async_runtime() {
    smol::Timer::after(Duration::from_millis(1)).await;
}

#[hotpath::measure]
fn sync_plain(input: u64) -> u64 {
    std::hint::black_box(input + 1)
}

#[hotpath::measure(log = true)]
fn sync_log(input: u64) -> u64 {
    std::hint::black_box(input + 2)
}

#[hotpath::measure]
async fn async_plain(input: u64) -> u64 {
    smol::Timer::after(Duration::from_millis(1)).await;
    std::hint::black_box(input + 3)
}

#[hotpath::measure(log = true)]
async fn async_log(input: u64) -> u64 {
    smol::Timer::after(Duration::from_millis(1)).await;
    std::hint::black_box(input + 4)
}

#[hotpath::measure(future = true)]
async fn async_future(input: u64) -> u64 {
    smol::Timer::after(Duration::from_millis(1)).await;
    std::hint::black_box(input + 5)
}

#[hotpath::measure(future = true, log = true)]
async fn async_future_log(input: u64) -> u64 {
    smol::Timer::after(Duration::from_millis(1)).await;
    std::hint::black_box(input + 6)
}

#[hotpath::main]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    smol::block_on(async {
        warm_up_async_runtime().await;

        let mut total = 0_u64;
        for i in 0..3_u64 {
            total += sync_plain(i);
            total += sync_log(i);
            total += async_log(i).await;
            total += async_plain(i).await;
            total += async_future(i).await;
            total += async_future_log(i).await;
        }
        std::hint::black_box(total);
        Ok(())
    })
}
