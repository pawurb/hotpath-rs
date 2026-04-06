use futures_util::stream::{self, StreamExt};

fn main() {
    smol::block_on(async {
        let _streams_guard = hotpath::HotpathGuardBuilder::new("main")
            .format(hotpath::Format::Json)
            .output_path("tmp/streams_output_test.json")
            .sections(vec![hotpath::Section::Streams])
            .build();

        let stream = hotpath::stream!(stream::iter(1..=5), label = "number-stream");

        println!("Collecting numbers...");
        let numbers: Vec<i32> = stream.collect().await;
        println!("Collected: {:?}", numbers);
    })
}
