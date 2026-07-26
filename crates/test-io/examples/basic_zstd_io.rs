//! Exercises `io!` over zstd compression streams with stacked wrappers: the
//! encoder writes through an instrumented `File` and is itself instrumented,
//! so the report shows plaintext application ops next to compressed on-disk
//! ops. Uses the same JSON fixture as the gzip example, so the two codecs'
//! ratios are directly comparable. Round-trips through wrapped decoders to
//! verify delegation.
//!
//! Run with:
//!   cargo run -p test-io --example basic_zstd_io --features hotpath

use std::fs::File;
use std::io::{Read, Write};

const CHUNK: usize = 1024;
const DATA: &[u8] = include_bytes!("../fixtures/records.json");
const DEFAULT_LEVEL: i32 = 3;

/// Compression level, overridable via `COMPRESSION_RATE` (zstd: 1-22).
fn level() -> i32 {
    std::env::var("COMPRESSION_RATE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_LEVEL)
}

fn main() {
    let _guard = hotpath::HotpathGuardBuilder::new("main")
        .sections(vec![hotpath::Section::Io])
        .build();

    let path = std::env::temp_dir().join("hotpath_basic_zstd_io.zst");
    let data = DATA;
    let level = level();

    // Inner wrapper measures compressed bytes hitting the file; outer wrapper
    // measures plaintext application writes.
    let file = hotpath::io!(
        File::create(&path).unwrap(),
        label = "zstd-compressed-write"
    );
    let mut encoder = hotpath::io!(
        zstd::stream::write::Encoder::new(file, level).unwrap(),
        label = "zstd-plaintext-write"
    );
    for chunk in data.chunks(CHUNK) {
        encoder.write_all(chunk).unwrap();
    }
    // finish(self) consumes the encoder, so peel the outer wrapper off with
    // io_unwrap first; the epilogue still flows through the inner file
    // wrapper, so its byte count covers the whole .zst file.
    hotpath::io_unwrap(encoder).finish().unwrap();

    // Ground truth to compare against the report: the plaintext size should
    // match the zstd-plaintext-write row, the file size the
    // zstd-compressed-write row.
    let compressed_len = std::fs::metadata(&path).unwrap().len();
    println!(
        "json fixture: {} B plaintext -> {} B zstd level {level} ({:.2}x compression)",
        data.len(),
        compressed_len,
        data.len() as f64 / compressed_len as f64
    );

    let file = hotpath::io!(File::open(&path).unwrap(), label = "zstd-compressed-read");
    let mut decoder = hotpath::io!(
        zstd::stream::read::Decoder::new(file).unwrap(),
        label = "zstd-plaintext-read"
    );
    let mut out = Vec::new();
    decoder.read_to_end(&mut out).unwrap();
    assert_eq!(out, data);

    std::fs::remove_file(&path).ok();
    println!("Zstd io example completed!");
}
