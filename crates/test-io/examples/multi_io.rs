//! Exercises `io!` across several stream kinds in one run - Redis TCP round
//! trips, a TLS connection to hotpath.rs, file writes and reads, and a gzip
//! codec - producing a multi-row I/O report. Redis requires the container from
//! the repo-root compose file (`docker compose up -d redis`, host port 6390);
//! the Redis and TLS sections are skipped when their target is unreachable.
//!
//! Run with:
//!   cargo run -p test-io --example multi_io --features hotpath

use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::Arc;

const REDIS_ITERS: usize = 2_000;
const REDIS_VALUE_SIZES: &[usize] = &[64, 512, 1024, 4096, 8192];
const REDIS_KEY: &[u8] = b"hotpath-multi-io";

const TLS_HOST: &str = "hotpath.rs";
const TLS_FETCHES: usize = 3;

const CHUNK_SIZES: &[usize] = &[256 * 1024, 512 * 1024, 1024 * 1024, 4 * 1024 * 1024];
const MAX_CHUNK: usize = 4 * 1024 * 1024;
const FILE_TOTAL: usize = 64 * 1024 * 1024;
const GZIP_TOTAL: usize = 16 * 1024 * 1024;

// Writes `total` bytes in cycling chunk sizes, so per-operation durations
// spread instead of collapsing into identical Avg and P95.
fn write_chunks(writer: &mut impl Write, chunk: &[u8], total: usize) {
    let (mut written, mut i) = (0, 0);
    while written < total {
        let size = CHUNK_SIZES[i % CHUNK_SIZES.len()];
        writer.write_all(&chunk[..size]).unwrap();
        written += size;
        i += 1;
    }
}

fn read_chunks(reader: &mut impl Read) {
    let mut buf = vec![0u8; MAX_CHUNK];
    for i in 0.. {
        let size = CHUNK_SIZES[i % CHUNK_SIZES.len()];
        if reader.read(&mut buf[..size]).unwrap() == 0 {
            break;
        }
    }
}

fn resp_cmd(parts: &[&[u8]]) -> Vec<u8> {
    let mut out = format!("*{}\r\n", parts.len()).into_bytes();
    for part in parts {
        out.extend_from_slice(format!("${}\r\n", part.len()).as_bytes());
        out.extend_from_slice(part);
        out.extend_from_slice(b"\r\n");
    }
    out
}

fn read_expected(stream: &mut impl Read, expected: &[u8]) {
    let mut buf = vec![0u8; expected.len()];
    stream.read_exact(&mut buf).unwrap();
    assert_eq!(buf, expected);
}

fn redis_section() {
    let addr = std::env::var("REDIS_ADDR").unwrap_or_else(|_| "127.0.0.1:6390".to_string());
    let Ok(stream) = TcpStream::connect(&addr) else {
        eprintln!(
            "skipping Redis section: no Redis on {addr} \
             (start it with `docker compose up -d redis`)"
        );
        return;
    };
    stream.set_nodelay(true).unwrap();

    let mut redis = hotpath::io!(stream, label = "redis");

    for i in 0..REDIS_ITERS {
        let size = REDIS_VALUE_SIZES[i % REDIS_VALUE_SIZES.len()];
        let value = vec![b'v'; size];

        redis
            .write_all(&resp_cmd(&[b"SET", REDIS_KEY, &value]))
            .unwrap();
        read_expected(&mut redis, b"+OK\r\n");

        redis.write_all(&resp_cmd(&[b"GET", REDIS_KEY])).unwrap();
        let expected = [format!("${size}\r\n").as_bytes(), &value, b"\r\n"].concat();
        read_expected(&mut redis, &expected);

        redis.write_all(&resp_cmd(&[b"PING"])).unwrap();
        read_expected(&mut redis, b"+PONG\r\n");
    }

    redis.write_all(&resp_cmd(&[b"DEL", REDIS_KEY])).unwrap();
    read_expected(&mut redis, b":1\r\n");
}

fn tls_section() {
    let mut roots = rustls::RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let config = Arc::new(
        rustls::ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth(),
    );

    for _ in 0..TLS_FETCHES {
        let Ok(mut sock) = TcpStream::connect((TLS_HOST, 443)) else {
            eprintln!("skipping TLS section: cannot reach {TLS_HOST}:443");
            return;
        };
        let server_name = rustls::pki_types::ServerName::try_from(TLS_HOST).unwrap();
        let mut conn = rustls::ClientConnection::new(config.clone(), server_name).unwrap();
        // Complete the handshake before wrapping; otherwise its round trips are
        // billed to the first application write.
        while conn.is_handshaking() {
            conn.complete_io(&mut sock).unwrap();
        }

        // Wraps the TLS stream itself, so the report measures plaintext bytes
        // as the application sees them, not the encrypted wire bytes.
        let mut tls = hotpath::io!(rustls::StreamOwned::new(conn, sock), label = "tls");

        tls.write_all(
            format!("GET / HTTP/1.1\r\nHost: {TLS_HOST}\r\nConnection: close\r\n\r\n").as_bytes(),
        )
        .unwrap();
        let mut response = Vec::new();
        tls.read_to_end(&mut response).unwrap();
        assert!(response.starts_with(b"HTTP/1.1"));
    }
}

fn file_section(chunk: &[u8]) {
    let path = std::env::temp_dir().join("hotpath_multi_io.bin");

    let mut file = hotpath::io!(std::fs::File::create(&path).unwrap(), label = "file");
    write_chunks(&mut file, chunk, FILE_TOTAL);
    drop(file);

    let mut file = hotpath::io!(std::fs::File::open(&path).unwrap(), label = "file");
    read_chunks(&mut file);
    drop(file);

    std::fs::remove_file(&path).unwrap();
}

fn gzip_section(chunk: &[u8]) {
    let mut encoder = hotpath::io!(
        flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default()),
        label = "gzip"
    );
    write_chunks(&mut encoder, chunk, GZIP_TOTAL);
    // finish(self) consumes the encoder, so peel the wrapper off with io_unwrap.
    let compressed = hotpath::io_unwrap(encoder).finish().unwrap();

    let mut decoder = hotpath::io!(
        flate2::read::GzDecoder::new(&compressed[..]),
        label = "gzip"
    );
    read_chunks(&mut decoder);
}

// Low-entropy data (16 symbols), so gzip compresses and decompresses it at
// realistic text-like speeds instead of degenerating into stored-block copies.
fn pseudo_random_chunk(len: usize) -> Vec<u8> {
    let mut state: u64 = 0x9E37_79B9_7F4A_7C15;
    (0..len)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            b'a' + (state as u8 & 0x0F)
        })
        .collect()
}

fn main() {
    let _guard = hotpath::HotpathGuardBuilder::new("main")
        .sections(vec![hotpath::Section::Io])
        .build();

    redis_section();
    tls_section();

    let chunk = pseudo_random_chunk(MAX_CHUNK);
    file_section(&chunk);
    gzip_section(&chunk);

    println!("multi_io example completed!");
}
