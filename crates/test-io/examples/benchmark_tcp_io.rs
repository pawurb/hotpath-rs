//! Measures `io!` overhead on loopback TCP echo round trips against an
//! in-process server - no external services required.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::thread;
use std::time::Instant;

const WARMUP: usize = 1_000;
const OPS: usize = 10_000;
const CHUNK: usize = 64;

fn spawn_echo_server() -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    thread::spawn(move || {
        // One connection per benchmarked stream; echo until the peer closes.
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { break };
            stream.set_nodelay(true).unwrap();
            thread::spawn(move || {
                let mut buf = [0u8; CHUNK];
                loop {
                    match stream.read(&mut buf) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => {
                            if stream.write_all(&buf[..n]).is_err() {
                                break;
                            }
                        }
                    }
                }
            });
        }
    });
    addr
}

fn connect(addr: SocketAddr) -> TcpStream {
    let stream = TcpStream::connect(addr).unwrap();
    stream.set_nodelay(true).unwrap();
    stream
}

fn echo_round_trips(stream: &mut (impl Read + Write)) -> f64 {
    let payload = [7u8; CHUNK];
    let mut buf = [0u8; CHUNK];
    for _ in 0..WARMUP {
        stream.write_all(&payload).unwrap();
        stream.read_exact(&mut buf).unwrap();
    }
    let start = Instant::now();
    for _ in 0..OPS {
        stream.write_all(&payload).unwrap();
        stream.read_exact(&mut buf).unwrap();
    }
    start.elapsed().as_nanos() as f64 / OPS as f64
}

fn main() {
    let _guard = hotpath::HotpathGuardBuilder::new("main")
        .sections(vec![hotpath::Section::Io])
        .build();

    let addr = spawn_echo_server();

    let mut raw = connect(addr);
    let raw_ns = echo_round_trips(&mut raw);
    drop(raw);

    let mut wrapped = hotpath::io!(connect(addr));
    let wrapped_ns = echo_round_trips(&mut wrapped);
    drop(wrapped);

    // Each round trip is one instrumented write plus at least one instrumented read.
    println!(
        "tcp echo: raw {raw_ns:.0} ns/op, wrapped {wrapped_ns:.0} ns/op, \
         overhead {:.0} ns/op ({:.2}%)",
        wrapped_ns - raw_ns,
        (wrapped_ns - raw_ns) / raw_ns * 100.0
    );
}
