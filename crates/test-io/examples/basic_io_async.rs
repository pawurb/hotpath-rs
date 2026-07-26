use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt, ReadBuf};

/// Async reader that always fails with a non-retryable error.
struct ErrReader;

impl tokio::io::AsyncRead for ErrReader {
    fn poll_read(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        _buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        Poll::Ready(Err(std::io::Error::other("async boom")))
    }
}

const SERVER_DELAY_MS: u64 = 200;

#[tokio::main]
async fn main() {
    let _guard = hotpath::HotpathGuardBuilder::new("main")
        .sections(vec![hotpath::Section::Io])
        .build();

    let data: Vec<u8> = (0u8..100).collect();
    let path = std::env::temp_dir().join("hotpath_basic_io_async.bin");

    // Write the fixture file: 10 operations x 10 bytes, then flush and shutdown.
    let mut writer = hotpath::io!(
        tokio::fs::File::create(&path).await.unwrap(),
        label = "fixture-write"
    );
    for chunk in data.chunks(10) {
        writer.write_all(chunk).await.unwrap();
    }
    writer.flush().await.unwrap();
    writer.shutdown().await.unwrap();
    drop(writer);

    // Read it back: tokio file reads run on the blocking pool, so poll_read
    // completes Pending -> Ready. Delegation verified byte-for-byte.
    let mut reader = hotpath::io!(
        tokio::fs::File::open(&path).await.unwrap(),
        label = "fixture-read"
    );
    let mut out = Vec::new();
    let mut buf = [0u8; 10];
    for _ in 0..10 {
        reader.read_exact(&mut buf).await.unwrap();
        out.extend_from_slice(&buf);
    }
    assert_eq!(out, data);

    // Duplex pair with a delayed peer: the client read spans Pending polls and
    // its measured duration includes the async waiting time.
    let (client, mut server) = tokio::io::duplex(64);
    let mut client = hotpath::io!(client, label = "duplex-client");

    let server_task = tokio::spawn(async move {
        let mut buf = [0u8; 5];
        server.read_exact(&mut buf).await.unwrap();
        assert_eq!(&buf, b"hello");
        tokio::time::sleep(Duration::from_millis(SERVER_DELAY_MS)).await;
        server.write_all(b"world back").await.unwrap();
    });

    client.write_all(b"hello").await.unwrap();
    client.flush().await.unwrap();

    let mut buf = [0u8; 10];
    client.read_exact(&mut buf).await.unwrap();
    assert_eq!(&buf, b"world back");

    client.shutdown().await.unwrap();
    server_task.await.unwrap();

    let mut err_reader = hotpath::io!(ErrReader, label = "err-reader");
    let mut sink = Vec::new();
    assert!(err_reader.read_to_end(&mut sink).await.is_err());

    tokio::fs::remove_file(&path).await.ok();
    println!("Async io example completed!");
}
