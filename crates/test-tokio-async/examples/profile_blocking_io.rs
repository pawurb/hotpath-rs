//! Blocking I/O profiling example
//!
//! Compares hotpath instrumentation vs sampling profilers.
//!
//! Profile with hotpath:
//! ```bash
//! cargo run --example profile_blocking_io --features hotpath --profile profiling
//! ```
//!
//! Profile with samply:
//! ```bash
//! cargo build --example profile_blocking_io --profile profiling
//! samply record ./target/profiling/examples/profile_blocking_io
//! ```

use std::fs::File;
use std::io::{BufReader, Read, Write};

const FILE_SIZE: usize = 10 * 1024 * 1024; // 10 MB
const CHUNK_SIZE: usize = 8 * 1024; // 8 KB

#[hotpath::measure]
fn create_test_file(path: &str) {
    let mut file = File::create(path).expect("create");
    let buf = vec![0xABu8; CHUNK_SIZE];

    for _ in 0..(FILE_SIZE / CHUNK_SIZE) {
        file.write_all(&buf).expect("write");
    }

    file.sync_all().expect("sync");
}

#[hotpath::measure]
fn read_file(path: &str) -> Vec<u8> {
    let file = File::open(path).expect("open");
    let mut reader = BufReader::with_capacity(CHUNK_SIZE, file);
    let mut data = Vec::with_capacity(FILE_SIZE);
    reader.read_to_end(&mut data).expect("read");
    data
}

#[hotpath::main]
fn main() {
    let path = "/tmp/hotpath_blocking.bin";
    create_test_file(path);

    for _ in 0..20 {
        let _data = read_file(path);
    }

    let _ = std::fs::remove_file(path);
}
