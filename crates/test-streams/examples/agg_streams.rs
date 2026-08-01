//! Run with:
//!   cargo run -p test-streams --example agg_streams --features hotpath

// Loop-created streams: the default mode aggregates all instances from one
// call site into a single entry; `iter = true` keeps one entry per instance
// with suffixed labels.

use futures_util::stream::{self, StreamExt};

fn main() {
    smol::block_on(async {
        let _streams_guard = hotpath::HotpathGuardBuilder::new("main")
            .format(hotpath::Format::JsonPretty)
            .sections(vec![hotpath::Section::Streams])
            .build();

        for _ in 0..4 {
            let s = hotpath::stream!(stream::iter(1..=5));
            let _items: Vec<_> = s.collect().await;
        }

        for _ in 0..3 {
            let s = hotpath::stream!(stream::iter(1..=2), label = "itered", iter = true);
            let _items: Vec<_> = s.collect().await;
        }

        drop(_streams_guard);

        println!("Aggregated streams example completed!");
    })
}
