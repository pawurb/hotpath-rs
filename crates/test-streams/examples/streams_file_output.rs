use futures_util::stream::{self, StreamExt};
use hotpath::streams::StreamsGuardBuilder;
use hotpath::Format;

fn main() {
    smol::block_on(async {
        let _streams_guard = StreamsGuardBuilder::new()
            .format(Format::Json)
            .output_path("tmp/streams_output_test.json")
            .build();

        let stream = hotpath::stream!(stream::iter(1..=5), label = "number-stream");

        println!("Collecting numbers...");
        let numbers: Vec<i32> = stream.collect().await;
        println!("Collected: {:?}", numbers);
    })
}
