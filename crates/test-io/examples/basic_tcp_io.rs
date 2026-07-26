//! Exercises `io!` over a real TCP connection: an in-process echo server on a
//! loopback listener, with the client stream instrumented. Self-contained -
//! no external services required.
//!
//! Run with:
//!   cargo run -p test-io --example basic_tcp_io --features hotpath

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;

const PAYLOAD_LEN: usize = 1024;
const ROUNDS: usize = 8;

fn main() {
    let _guard = hotpath::HotpathGuardBuilder::new("main")
        .sections(vec![hotpath::Section::Io])
        .build();

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();

    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut buf = [0u8; PAYLOAD_LEN];
        for _ in 0..ROUNDS {
            stream.read_exact(&mut buf).unwrap();
            stream.write_all(&buf).unwrap();
        }
    });

    let mut client = hotpath::io!(TcpStream::connect(addr).unwrap(), label = "tcp-client");
    let payload: Vec<u8> = (0..PAYLOAD_LEN).map(|i| i as u8).collect();
    let mut echo = vec![0u8; PAYLOAD_LEN];
    for _ in 0..ROUNDS {
        client.write_all(&payload).unwrap();
        client.read_exact(&mut echo).unwrap();
        assert_eq!(echo, payload);
    }

    server.join().unwrap();
    println!("TCP io example completed!");
}
