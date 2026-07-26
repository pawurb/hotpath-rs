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

/// Async reader that fails with retryable `Interrupted` on its first call,
/// then yields data; retryable errors must count as neither ops nor errors.
struct FlakyReader {
    calls: u32,
}

impl tokio::io::AsyncRead for FlakyReader {
    fn poll_read(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        self.calls += 1;
        match self.calls {
            1 => Poll::Ready(Err(std::io::Error::from(std::io::ErrorKind::Interrupted))),
            2 => {
                buf.put_slice(b"flaky");
                Poll::Ready(Ok(()))
            }
            _ => Poll::Ready(Ok(())),
        }
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

    // Retryable Interrupted surfaces to the caller but is not recorded; the
    // retried read is a fresh operation with its own timing span.
    let mut flaky = hotpath::io!(FlakyReader { calls: 0 }, label = "flaky-reader");
    let mut fbuf = [0u8; 5];
    assert_eq!(
        flaky.read(&mut fbuf).await.unwrap_err().kind(),
        std::io::ErrorKind::Interrupted
    );
    flaky.read_exact(&mut fbuf).await.unwrap();
    assert_eq!(&fbuf, b"flaky");

    tokio::fs::remove_file(&path).await.ok();
    println!("Async io example completed!");
}
