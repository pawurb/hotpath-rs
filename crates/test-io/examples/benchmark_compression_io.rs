//! Measures `io!` overhead on compression streams: raw vs instrumented
//! encoders across codecs and chunk sizes. The wrapper cost is fixed per
//! write op, so the relative overhead depends on how much compression work
//! each chunk carries - gzip on large chunks buries it, a fast codec on small
//! chunks is the worst case.
//!
//! Run with:
//!   cargo run -p test-io --example benchmark_compression_io --features hotpath

use flate2::write::GzEncoder;
use flate2::Compression;
use std::io::Write;
use std::time::Instant;

const TOTAL_BYTES: usize = 8 * 1024 * 1024;
const CHUNK_SIZES: &[usize] = &[64, 4096];

fn make_data() -> Vec<u8> {
    // Repetitive pattern so the codecs do real compression work.
    (0..TOTAL_BYTES).map(|i| (i % 97) as u8).collect()
}

fn timed_writes(sink: &mut impl Write, data: &[u8], chunk: usize) -> f64 {
    let ops = data.len() / chunk;
    let start = Instant::now();
    for c in data.chunks(chunk) {
        sink.write_all(c).unwrap();
    }
    start.elapsed().as_nanos() as f64 / ops as f64
}

fn report(codec: &str, chunk: usize, raw_ns: f64, wrapped_ns: f64) {
    println!(
        "{codec} {chunk}B chunks: raw {raw_ns:.0} ns/op, wrapped {wrapped_ns:.0} ns/op, \
         overhead {:.0} ns/op ({:.2}%)",
        wrapped_ns - raw_ns,
        (wrapped_ns - raw_ns) / raw_ns * 100.0
    );
}

fn main() {
    let _guard = hotpath::HotpathGuardBuilder::new("main")
        .sections(vec![hotpath::Section::Io])
        .build();

    let data = make_data();

    for &chunk in CHUNK_SIZES {
        // The sink discards output, so encoders are dropped unfinished; only
        // the timed writes matter.
        let mut raw = GzEncoder::new(std::io::sink(), Compression::default());
        let raw_ns = timed_writes(&mut raw, &data, chunk);
        drop(raw);

        let mut wrapped = hotpath::io!(
            GzEncoder::new(std::io::sink(), Compression::default()),
            label = "gzip"
        );
        let wrapped_ns = timed_writes(&mut wrapped, &data, chunk);
        drop(wrapped);

        report("gzip", chunk, raw_ns, wrapped_ns);
    }

    for &chunk in CHUNK_SIZES {
        let mut raw = zstd::stream::write::Encoder::new(std::io::sink(), 3)
            .unwrap()
            .auto_finish();
        let raw_ns = timed_writes(&mut raw, &data, chunk);
        drop(raw);

        let mut wrapped = hotpath::io!(
            zstd::stream::write::Encoder::new(std::io::sink(), 3)
                .unwrap()
                .auto_finish(),
            label = "zstd"
        );
        let wrapped_ns = timed_writes(&mut wrapped, &data, chunk);
        drop(wrapped);

        report("zstd", chunk, raw_ns, wrapped_ns);
    }
}
