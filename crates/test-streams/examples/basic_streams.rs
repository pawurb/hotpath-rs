use futures_util::stream::{self, StreamExt};
use smol::Timer;
use std::time::Duration;

#[allow(unused_mut)]
fn main() {
    smol::block_on(async {
        #[cfg(feature = "hotpath")]
        let _channels_guard = hotpath::channels::ChannelsGuard::new();

        // Example 1: Basic stream from iterator
        let stream = stream::iter(1..=5);
        #[cfg(feature = "hotpath")]
        let stream = hotpath::stream!(stream, label = "number-stream");

        println!("[Stream 1] Collecting numbers...");
        let numbers: Vec<i32> = stream.collect().await;
        println!("[Stream 1] Collected: {:?}", numbers);

        // Example 2: Stream with logging enabled
        let stream2 = stream::iter(vec!["hello", "world", "from", "streams"]);
        #[cfg(feature = "hotpath")]
        let stream2 = hotpath::stream!(stream2, label = "text-stream", log = true);

        println!("\n[Stream 2] Processing text...");
        stream2
            .for_each(|text| async move {
                println!("[Stream 2] Yielded: {}", text);
                Timer::after(Duration::from_millis(100)).await;
            })
            .await;

        // Example 3: Infinite stream (take first 3)
        let stream3 = stream::repeat(42).take(3);
        #[cfg(feature = "hotpath")]
        let stream3 = hotpath::stream!(stream3, label = "repeat-stream");

        println!("\n[Stream 3] Taking from infinite stream...");
        let repeated: Vec<i32> = stream3.collect().await;
        println!("[Stream 3] Collected: {:?}", repeated);

        println!("\nStream example completed!");

        // Give stats collector time to process final events
        Timer::after(Duration::from_millis(100)).await;
    })
}
