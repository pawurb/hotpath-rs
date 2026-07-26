//! Exercises `io!` instrumentation over a real Redis connection: a fixed
//! SET/GET/PING/DEL sequence over an instrumented `TcpStream`, speaking raw
//! RESP so no client dependency is needed. Requires the Redis container from
//! the repo-root compose file (`docker compose up -d redis`, host port 6390);
//! skips when nothing listens there.

use std::io::{Read, Write};
use std::net::TcpStream;

const SET: &[u8] = b"*3\r\n$3\r\nSET\r\n$11\r\nhotpath-key\r\n$11\r\nhotpath-val\r\n";
const GET: &[u8] = b"*2\r\n$3\r\nGET\r\n$11\r\nhotpath-key\r\n";
const PING: &[u8] = b"*1\r\n$4\r\nPING\r\n";
const DEL: &[u8] = b"*2\r\n$3\r\nDEL\r\n$11\r\nhotpath-key\r\n";

fn read_response(stream: &mut impl Read, expected: &[u8]) {
    let mut buf = vec![0u8; expected.len()];
    stream.read_exact(&mut buf).unwrap();
    assert_eq!(buf, expected);
}

fn main() {
    let addr = std::env::var("REDIS_ADDR").unwrap_or_else(|_| "127.0.0.1:6390".to_string());
    let Ok(stream) = TcpStream::connect(&addr) else {
        eprintln!(
            "skipping basic_redis_io: no Redis on {addr} \
             (start it with `docker compose up -d redis`)"
        );
        return;
    };

    let _guard = hotpath::HotpathGuardBuilder::new("main")
        .sections(vec![hotpath::Section::Io])
        .build();

    let mut redis = hotpath::io!(stream, label = "redis");

    redis.write_all(SET).unwrap();
    read_response(&mut redis, b"+OK\r\n");

    redis.write_all(GET).unwrap();
    read_response(&mut redis, b"$11\r\nhotpath-val\r\n");

    redis.write_all(PING).unwrap();
    read_response(&mut redis, b"+PONG\r\n");

    redis.write_all(DEL).unwrap();
    read_response(&mut redis, b":1\r\n");

    println!("Redis io example completed!");
}
