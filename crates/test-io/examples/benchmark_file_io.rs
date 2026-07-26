//! Run with:
//!   cargo run -p test-io --example benchmark_file_io --features hotpath

use std::fs::File;
use std::io::{Read, Write};
use std::time::Instant;

const OPS: usize = 200_000;
const CHUNK: usize = 64;

fn main() {
    let _guard = hotpath::HotpathGuardBuilder::new("main")
        .sections(vec![hotpath::Section::Io])
        .build();

    let dir = std::env::temp_dir();
    let raw_path = dir.join("hotpath_bench_raw.bin");
    let wrapped_path = dir.join("hotpath_bench_wrapped.bin");
    let buf = [7u8; CHUNK];
    let mut rbuf = [0u8; CHUNK];

    let mut raw_writer = File::create(&raw_path).unwrap();
    let start = Instant::now();
    for _ in 0..OPS {
        raw_writer.write_all(&buf).unwrap();
    }
    let raw_write_ns = start.elapsed().as_nanos() as f64 / OPS as f64;
    drop(raw_writer);

    let mut writer = hotpath::io!(File::create(&wrapped_path).unwrap(), label = "bench-write");
    let start = Instant::now();
    for _ in 0..OPS {
        writer.write_all(&buf).unwrap();
    }
    let write_ns = start.elapsed().as_nanos() as f64 / OPS as f64;
    drop(writer);

    let mut raw_reader = File::open(&raw_path).unwrap();
    let start = Instant::now();
    for _ in 0..OPS {
        raw_reader.read_exact(&mut rbuf).unwrap();
        std::hint::black_box(&rbuf);
    }
    let raw_read_ns = start.elapsed().as_nanos() as f64 / OPS as f64;

    let mut reader = hotpath::io!(File::open(&wrapped_path).unwrap(), label = "bench-read");
    let start = Instant::now();
    for _ in 0..OPS {
        reader.read_exact(&mut rbuf).unwrap();
        std::hint::black_box(&rbuf);
    }
    let read_ns = start.elapsed().as_nanos() as f64 / OPS as f64;

    std::fs::remove_file(&raw_path).ok();
    std::fs::remove_file(&wrapped_path).ok();

    println!(
        "reads:  raw {raw_read_ns:.1} ns/op, wrapped {read_ns:.1} ns/op, overhead {:.1} ns/op ({:.1}%)",
        read_ns - raw_read_ns,
        (read_ns - raw_read_ns) / raw_read_ns * 100.0
    );
    println!(
        "writes: raw {raw_write_ns:.1} ns/op, wrapped {write_ns:.1} ns/op, overhead {:.1} ns/op ({:.1}%)",
        write_ns - raw_write_ns,
        (write_ns - raw_write_ns) / raw_write_ns * 100.0
    );
}
