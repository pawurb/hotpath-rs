//! Measures `io!` overhead on tokio async TCP echo round trips against an
//! in-process server - no external services required.
//!
//! Run with:
//!   cargo run -p test-io --example benchmark_tokio_tcp_io --features hotpath

use std::time::Instant;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

const WARMUP: usize = 1_000;
const OPS: usize = 10_000;
const CHUNK: usize = 64;

async fn spawn_echo_server() -> std::net::SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        // One connection per benchmarked stream; echo until the peer closes.
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                break;
            };
            stream.set_nodelay(true).unwrap();
            tokio::spawn(async move {
                let mut buf = [0u8; CHUNK];
                loop {
                    match stream.read(&mut buf).await {
                        Ok(0) | Err(_) => break,
                        Ok(n) => {
                            if stream.write_all(&buf[..n]).await.is_err() {
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

async fn connect(addr: std::net::SocketAddr) -> TcpStream {
    let stream = TcpStream::connect(addr).await.unwrap();
    stream.set_nodelay(true).unwrap();
    stream
}

async fn echo_round_trips(stream: &mut (impl AsyncRead + AsyncWrite + Unpin)) -> f64 {
    let payload = [7u8; CHUNK];
    let mut buf = [0u8; CHUNK];
    for _ in 0..WARMUP {
        stream.write_all(&payload).await.unwrap();
        stream.read_exact(&mut buf).await.unwrap();
    }
    let start = Instant::now();
    for _ in 0..OPS {
        stream.write_all(&payload).await.unwrap();
        stream.read_exact(&mut buf).await.unwrap();
    }
    start.elapsed().as_nanos() as f64 / OPS as f64
}

#[tokio::main]
async fn main() {
    let _guard = hotpath::HotpathGuardBuilder::new("main")
        .sections(vec![hotpath::Section::Io])
        .build();

    let addr = spawn_echo_server().await;

    let mut raw = connect(addr).await;
    let raw_ns = echo_round_trips(&mut raw).await;
    drop(raw);

    let mut wrapped = hotpath::io!(connect(addr).await);
    let wrapped_ns = echo_round_trips(&mut wrapped).await;
    drop(wrapped);

    // Each round trip is one instrumented write plus at least one instrumented read.
    println!(
        "tokio tcp echo: raw {raw_ns:.0} ns/op, wrapped {wrapped_ns:.0} ns/op, \
         overhead {:.0} ns/op ({:.2}%)",
        wrapped_ns - raw_ns,
        (wrapped_ns - raw_ns) / raw_ns * 100.0
    );
}
