//! Shows how the io `Rate` column behaves when concurrent readers share one
//! `io!` call site. Each read delivers 1 MiB after a fixed 10ms delay, i.e.
//! ~100 MiB/s per reader; the `sequential` entry is a single reader while
//! `concurrent` aggregates four fully-overlapped readers, so comparing the
//! two rows shows how overlapped operation time is attributed. The
//! `per-reader` site uses `iter = true`, giving each of its four readers a
//! separate row with individual rate and byte counts.
//!
//! Run with:
//!   cargo run -p test-io --example concurrent_io --features hotpath

use std::io::Read;
use std::thread;
use std::time::Duration;

const CHUNK: usize = 1024 * 1024;
const OPS_PER_READER: usize = 25;
const OP_DELAY: Duration = Duration::from_millis(10);
const READERS: usize = 4;

struct SlowReader;

impl Read for SlowReader {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        thread::sleep(OP_DELAY);
        Ok(buf.len())
    }
}

fn main() {
    let _guard = hotpath::HotpathGuardBuilder::new("main")
        .sections(vec![hotpath::Section::Io])
        .build();

    let mut buf = vec![0u8; CHUNK];
    let mut reader = hotpath::io!(SlowReader, label = "sequential");
    for _ in 0..OPS_PER_READER {
        let n = reader.read(&mut buf).unwrap();
        assert_eq!(n, CHUNK);
    }

    thread::scope(|s| {
        for _ in 0..READERS {
            s.spawn(|| {
                let mut buf = vec![0u8; CHUNK];
                let mut reader = hotpath::io!(SlowReader, label = "concurrent");
                for _ in 0..OPS_PER_READER {
                    let n = reader.read(&mut buf).unwrap();
                    assert_eq!(n, CHUNK);
                }
            });
        }
    });

    thread::scope(|s| {
        for _ in 0..READERS {
            s.spawn(|| {
                let mut buf = vec![0u8; CHUNK];
                let mut reader = hotpath::io!(SlowReader, label = "per-reader", iter = true);
                for _ in 0..OPS_PER_READER {
                    let n = reader.read(&mut buf).unwrap();
                    assert_eq!(n, CHUNK);
                }
            });
        }
    });

    println!("Concurrent io example completed!");
}
