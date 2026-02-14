use futures_util::stream::{self, StreamExt};
use smol::Timer;
use std::time::Duration;

fn main() {
    smol::block_on(async {
        hotpath::streams::StreamsGuardBuilder::new().build_with_timeout(Duration::from_secs(1));

        loop {
            let stream = hotpath::stream!(stream::iter(0..32), label = "timeout-stream");
            let _: Vec<i32> = stream.collect().await;
            Timer::after(Duration::from_millis(5)).await;
        }
    });
}
