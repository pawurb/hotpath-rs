//! Async I/O profiling long running example
//!
//! Compares hotpath instrumentation vs sampling profilers.
//!
//! Profile with hotpath:
//! ```bash
//! cargo run --example profile_async_io_long --features hotpath --profile profiling
//! ```
//!
//! Profile with samply:
//! ```bash
//! cargo build --example profile_async_io_long --profile profiling
//! samply record ./target/profiling/examples/profile_async_io_long
//! ```

use futures_util::future::join_all;
use std::time::{Duration, Instant};
use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const FILE_SIZE: usize = 200 * 1024 * 1024; // 200 MB
const CHUNK_SIZE: usize = 8 * 1024; // 8 KB
const NUM_FILES: usize = 5;
const RUN_DURATION: Duration = Duration::from_secs(60);

#[hotpath::measure]
async fn create_file(path: &str) {
    let mut file = File::create(path).await.expect("create");
    let buf = vec![0xABu8; CHUNK_SIZE];
    for _ in 0..(FILE_SIZE / CHUNK_SIZE) {
        file.write_all(&buf).await.expect("write");
    }
    file.sync_all().await.expect("sync");
}

#[hotpath::measure]
async fn read_file(path: &str) -> Vec<u8> {
    let file = File::open(path).await.expect("open");
    let mut reader = tokio::io::BufReader::new(file);
    let mut data = Vec::with_capacity(FILE_SIZE);
    reader.read_to_end(&mut data).await.expect("read");
    data
}

#[tokio::main(flavor = "current_thread")]
#[hotpath::main]
async fn main() {
    let paths: Vec<String> = (0..NUM_FILES)
        .map(|i| format!("/tmp/hotpath_async_{i}.bin"))
        .collect();
    let path_refs: Vec<&str> = paths.iter().map(|s| s.as_str()).collect();

    let start = Instant::now();
    let mut iteration = 0u64;

    println!("Running for {} seconds...", RUN_DURATION.as_secs());

    while start.elapsed() < RUN_DURATION {
        iteration += 1;

        let futures: Vec<_> = path_refs.iter().map(|p| create_file(p)).collect();
        join_all(futures).await;

        let futures: Vec<_> = path_refs.iter().map(|p| read_file(p)).collect();
        join_all(futures).await;

        if iteration.is_multiple_of(10) {
            println!(
                "Iteration {}, elapsed: {:.1}s",
                iteration,
                start.elapsed().as_secs_f32()
            );
        }
    }

    for path in &paths {
        tokio::fs::remove_file(path).await.ok();
    }
}
