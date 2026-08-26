//! Run with:
//!   cargo run -p test-io --example same_line_io --features hotpath

// Two `io!` invocations on one physical line wrapping the same concrete type:
// the registration key includes the column, so each call site keeps its own
// entry (labels, byte counts) instead of aliasing into the first invocation's
// id.

use std::io::{Cursor, Read};

fn main() {
    let _guard = hotpath::HotpathGuardBuilder::new("main")
        .sections(vec![hotpath::Section::Io])
        .build();

    let data_a: Vec<u8> = (0u8..30).collect();
    let data_b: Vec<u8> = (0u8..70).collect();

    #[rustfmt::skip]
    let (mut reader_a, mut reader_b) = (hotpath::io!(Cursor::new(data_a), label = "same-line-a"), hotpath::io!(Cursor::new(data_b), label = "same-line-b"));

    let mut buf_a = [0u8; 30];
    reader_a.read_exact(&mut buf_a).expect("read a");
    let mut buf_b = [0u8; 70];
    reader_b.read_exact(&mut buf_b).expect("read b");

    println!("Same-line io example completed!");
}
