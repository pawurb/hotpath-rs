//! Measures `io!` overhead on real network I/O: raw RESP PING/SET/GET round
//! trips to a local Redis over a plain `TcpStream` vs an instrumented one.
//! Requires the Redis container from the repo-root compose file
//! (`docker compose up -d redis`, host port 6390); skips when nothing listens
//! there.
//!
//! Run with:
//!   cargo run -p test-io --example benchmark_redis_io --features hotpath

use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Instant;

const WARMUP: usize = 1_000;
const OPS: usize = 10_000;

const PING: &[u8] = b"*1\r\n$4\r\nPING\r\n";
const SET: &[u8] = b"*3\r\n$3\r\nSET\r\n$9\r\nbench-key\r\n$11\r\nbench-value\r\n";
const GET: &[u8] = b"*2\r\n$3\r\nGET\r\n$9\r\nbench-key\r\n";
const DEL: &[u8] = b"*2\r\n$3\r\nDEL\r\n$9\r\nbench-key\r\n";

// SET runs before GET, so the key is always present when GET is measured.
const CASES: &[(&str, &[u8], &[u8])] = &[
    ("PING", PING, b"+PONG\r\n"),
    ("SET", SET, b"+OK\r\n"),
    ("GET", GET, b"$11\r\nbench-value\r\n"),
];

fn connect(addr: &str) -> Option<TcpStream> {
    let stream = TcpStream::connect(addr).ok()?;
    stream.set_nodelay(true).ok()?;
    Some(stream)
}

fn round_trip(stream: &mut (impl Read + Write), request: &[u8], response: &[u8]) {
    stream.write_all(request).unwrap();
    let mut buf = vec![0u8; response.len()];
    stream.read_exact(&mut buf).unwrap();
    assert_eq!(buf, response);
}

fn round_trips(stream: &mut (impl Read + Write), request: &[u8], response: &[u8]) -> f64 {
    for _ in 0..WARMUP {
        round_trip(stream, request, response);
    }
    let start = Instant::now();
    for _ in 0..OPS {
        round_trip(stream, request, response);
    }
    start.elapsed().as_nanos() as f64 / OPS as f64
}

fn main() {
    let addr = std::env::var("REDIS_ADDR").unwrap_or_else(|_| "127.0.0.1:6390".to_string());

    let Some(mut raw) = connect(&addr) else {
        eprintln!(
            "skipping benchmark_redis_io: no Redis on {addr} \
             (start it with `docker compose up -d redis`)"
        );
        return;
    };

    let _guard = hotpath::HotpathGuardBuilder::new("main")
        .sections(vec![hotpath::Section::Io])
        .build();

    let mut wrapped = hotpath::io!(connect(&addr).expect("second redis connection"));

    // Each round trip is one instrumented write plus at least one instrumented read.
    for (name, request, response) in CASES {
        let raw_ns = round_trips(&mut raw, request, response);
        let wrapped_ns = round_trips(&mut wrapped, request, response);
        println!(
            "redis {name}: raw {raw_ns:.0} ns/op, wrapped {wrapped_ns:.0} ns/op, \
             overhead {:.0} ns/op ({:.2}%)",
            wrapped_ns - raw_ns,
            (wrapped_ns - raw_ns) / raw_ns * 100.0
        );
    }

    round_trip(&mut raw, DEL, b":1\r\n");
}
