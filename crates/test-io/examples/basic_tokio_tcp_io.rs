//! Exercises `io!` over tokio's async TCP: an in-process tokio echo server on
//! a loopback listener, with the client stream instrumented. Reads genuinely
//! suspend on the reactor until the echo arrives, so this covers the
//! Pending-to-Ready path on a real socket. Self-contained - no external
//! services required.
//!
//! Run with:
//!   cargo run -p test-io --example basic_tokio_tcp_io --features hotpath

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

const PAYLOAD_LEN: usize = 1024;
const ROUNDS: usize = 8;

#[tokio::main]
async fn main() {
    let _guard = hotpath::HotpathGuardBuilder::new("main")
        .sections(vec![hotpath::Section::Io])
        .build();

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut buf = [0u8; PAYLOAD_LEN];
        for _ in 0..ROUNDS {
            stream.read_exact(&mut buf).await.unwrap();
            stream.write_all(&buf).await.unwrap();
        }
    });

    let mut client = hotpath::io!(
        TcpStream::connect(addr).await.unwrap(),
        label = "tokio-tcp-client"
    );
    let payload: Vec<u8> = (0..PAYLOAD_LEN).map(|i| i as u8).collect();
    let mut echo = vec![0u8; PAYLOAD_LEN];
    for _ in 0..ROUNDS {
        client.write_all(&payload).await.unwrap();
        client.read_exact(&mut echo).await.unwrap();
        assert_eq!(echo, payload);
    }

    client.shutdown().await.unwrap();
    server.await.unwrap();
    println!("Tokio TCP io example completed!");
}
