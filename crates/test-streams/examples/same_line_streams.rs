//! Run with:
//!   cargo run -p test-streams --example same_line_streams --features hotpath

// Two `stream!` invocations on one physical line with the same item type: the
// registration key includes the column, so each call site keeps its own entry
// (labels, yield counts, lifecycle) instead of aliasing into the first
// invocation's id.

use futures_util::stream::{self, StreamExt};

fn main() {
    smol::block_on(async {
        let _streams_guard = hotpath::HotpathGuardBuilder::new("main")
            .format(hotpath::Format::JsonPretty)
            .sections(vec![hotpath::Section::Streams])
            .build();

        #[rustfmt::skip]
        let (a, b) = (hotpath::stream!(stream::iter(1..=3), label = "same-line-a"), hotpath::stream!(stream::iter(1..=7), label = "same-line-b"));

        let _items_a: Vec<_> = a.collect().await;
        let _items_b: Vec<_> = b.collect().await;

        drop(_streams_guard);

        println!("Same-line streams example completed!");
    })
}
