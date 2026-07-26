//! Exercises `io!` over compression streams with stacked wrappers: the gzip
//! encoder writes through an instrumented `File` and is itself instrumented,
//! so the report shows plaintext application ops next to compressed on-disk
//! ops - the compression ratio falls out of the two Bytes columns. The fixture
//! is a JSON array of varied records (`fixtures/records.json`), so the ratio
//! is realistic rather than a degenerate repeating pattern. Round-trips
//! through wrapped decoders to verify delegation.
//!
//! Run with:
//!   cargo run -p test-io --example basic_gzip_io --features hotpath

use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;
use std::fs::File;
use std::io::{Read, Write};

const CHUNK: usize = 1024;
const DATA: &[u8] = include_bytes!("../fixtures/records.json");

/// Compression level, overridable via `COMPRESSION_RATE` (gzip: 0-9).
fn level() -> Compression {
    std::env::var("COMPRESSION_RATE")
        .ok()
        .and_then(|v| v.parse().ok())
        .map(Compression::new)
        .unwrap_or_default()
}

fn main() {
    let _guard = hotpath::HotpathGuardBuilder::new("main")
        .sections(vec![hotpath::Section::Io])
        .build();

    let path = std::env::temp_dir().join("hotpath_basic_gzip_io.gz");
    let data = DATA;
    let level = level();

    // Inner wrapper measures compressed bytes hitting the file; outer wrapper
    // measures plaintext application writes.
    let file = hotpath::io!(
        File::create(&path).unwrap(),
        label = "gzip-compressed-write"
    );
    let mut encoder = hotpath::io!(GzEncoder::new(file, level), label = "gzip-plaintext-write");
    for chunk in data.chunks(CHUNK) {
        encoder.write_all(chunk).unwrap();
    }
    // Finish the gzip stream through the wrapper's Deref (try_finish takes
    // &mut self); the trailer still flows through the inner file wrapper, so
    // its byte count covers the whole .gz file.
    encoder.try_finish().unwrap();
    drop(encoder);

    // Ground truth to compare against the report: the plaintext size should
    // match the gzip-plaintext-write row, the file size the
    // gzip-compressed-write row.
    let compressed_len = std::fs::metadata(&path).unwrap().len();
    println!(
        "json fixture: {} B plaintext -> {} B gzip level {} ({:.2}x compression)",
        data.len(),
        compressed_len,
        level.level(),
        data.len() as f64 / compressed_len as f64
    );

    let file = hotpath::io!(File::open(&path).unwrap(), label = "gzip-compressed-read");
    let mut decoder = hotpath::io!(GzDecoder::new(file), label = "gzip-plaintext-read");
    let mut out = Vec::new();
    decoder.read_to_end(&mut out).unwrap();
    assert_eq!(out, data);

    std::fs::remove_file(&path).ok();
    println!("Gzip io example completed!");
}
