//! Compares gzip and brotli across five compression levels each. Every level
//! wraps two `io!` instruments: the encoder (uncompressed bytes in, so Rate is
//! compression speed) and its sink `Vec` (compressed bytes out), so the report
//! pairs `<codec>-<level>` with `<codec>-<level>-out` and the Bytes columns
//! show the compression ratio.
//!
//! Run with:
//!   cargo run --release -p test-io --example compression_levels_io --features hotpath

use std::io::Write;

const LEVELS: &[u32] = &[1, 3, 5, 7, 9];
const TOTAL: usize = 8 * 1024 * 1024;
const CHUNK_SIZES: &[usize] = &[256 * 1024, 512 * 1024, 1024 * 1024];

// Writes the corpus front to back in cycling chunk sizes, so per-operation
// durations spread instead of collapsing into identical Avg and P95.
fn write_corpus(writer: &mut impl Write, corpus: &[u8]) {
    let (mut off, mut i) = (0, 0);
    while off < corpus.len() {
        let size = CHUNK_SIZES[i % CHUNK_SIZES.len()].min(corpus.len() - off);
        writer.write_all(&corpus[off..off + size]).unwrap();
        off += size;
        i += 1;
    }
}

// A non-repeating pseudo-random word stream: text-like LZ structure without
// whole-buffer repeats, so ratios reflect the compression level rather than
// the codecs' window sizes.
fn word_corpus(len: usize) -> Vec<u8> {
    const WORDS: &[&str] = &[
        "alloc",
        "bytes",
        "channel",
        "future",
        "guard",
        "hotpath",
        "latency",
        "lock",
        "measure",
        "metrics",
        "profile",
        "queue",
        "report",
        "runtime",
        "sample",
        "socket",
        "stream",
        "thread",
        "throughput",
        "worker",
    ];
    let mut state: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut out = Vec::with_capacity(len + 16);
    while out.len() < len {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        out.extend_from_slice(WORDS[state as usize % WORDS.len()].as_bytes());
        out.push(b' ');
    }
    out.truncate(len);
    out
}

fn main() {
    let _guard = hotpath::HotpathGuardBuilder::new("main")
        .sections(vec![hotpath::Section::Io])
        .build();

    let corpus = word_corpus(TOTAL);

    for &level in LEVELS {
        let mut encoder = hotpath::io!(
            flate2::write::GzEncoder::new(
                hotpath::io!(Vec::new(), label = format!("gzip-{level}-out"), iter = true),
                flate2::Compression::new(level),
            ),
            label = format!("gzip-{level}"),
            iter = true
        );
        write_corpus(&mut encoder, &corpus);
        // Compress the buffered tail while still instrumented; finish(self)
        // then only writes the epilogue, outside the profiler's sight.
        encoder.flush().unwrap();
        hotpath::io_unwrap(hotpath::io_unwrap(encoder).finish().unwrap());
    }

    for &level in LEVELS {
        let mut encoder = hotpath::io!(
            brotli::CompressorWriter::new(
                hotpath::io!(
                    Vec::new(),
                    label = format!("brotli-{level}-out"),
                    iter = true
                ),
                4096,
                level,
                22,
            ),
            label = format!("brotli-{level}"),
            iter = true
        );
        write_corpus(&mut encoder, &corpus);
        encoder.flush().unwrap();
        hotpath::io_unwrap(hotpath::io_unwrap(encoder).into_inner());
    }

    println!("compression_levels_io example completed!");
}
