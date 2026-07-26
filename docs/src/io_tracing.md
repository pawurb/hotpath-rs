# Byte-level I/O monitoring

`hotpath` instruments byte-level I/O to surface slow reads and writes - files, sockets, TLS streams, compression streams, and any custom I/O type. For every wrapped value it tracks, per operation kind (read, write, flush, shutdown):

- **Operation count** - completed operations
- **Bytes processed** - total bytes read or written
- **Duration** - average and configured percentiles. Synchronous operations measure the full method call; async operations measure from the first poll to `Ready`, so reported durations include async waiting time (e.g. waiting for a socket to become readable), not only the final poll execution.
- **Errors** - failed operations. Retryable conditions (`WouldBlock`, `Interrupted`) are not counted as errors and produce no operation.

The `io!` macro is noop unless the `hotpath` feature is activated.

## io! macro

Wrap any value implementing `std::io::Read`, `std::io::Write`, `tokio::io::AsyncRead`, or `tokio::io::AsyncWrite` (async traits require the `tokio` feature). The wrapper delegates every operation to the wrapped value, so it is a drop-in replacement:

```rust
use std::io::Read;

let mut file = hotpath::io!(std::fs::File::open("data.bin")?, label = "data-file");
let mut buf = Vec::new();
file.read_to_end(&mut buf)?;
```

Async I/O works the same way:

```rust
use tokio::io::AsyncWriteExt;

let stream = tokio::net::TcpStream::connect("127.0.0.1:8080").await?;
let mut stream = hotpath::io!(stream, label = "backend");
stream.write_all(b"ping").await?;
```

The `label` parameter is optional; without it the wrapper is identified by `file:line`.

## What you measure depends on what you wrap

Wrapping the underlying resource (a `File`, `TcpStream`, or TLS stream) measures actual resource I/O: every syscall-level read and write, with buffering layers above it invisible.

Wrapping a `BufReader` or `BufWriter` instead measures application-facing buffered operations: many small reads served from the buffer, with the occasional large refill hidden inside them.

```rust
// Measures actual file I/O (large, infrequent reads).
let file = hotpath::io!(std::fs::File::open("data.bin")?, label = "file-raw");
let mut reader = std::io::BufReader::new(file);

// Measures application-facing reads (small, frequent, mostly buffer hits).
let mut reader = hotpath::io!(
    std::io::BufReader::new(std::fs::File::open("data.bin")?),
    label = "file-buffered"
);
```

## Cancellation caveat

Async operation durations span from the first poll to `Ready`. If an operation's future is cancelled mid-`Pending` (e.g. a `tokio::select!` timeout drops a `read()` future), the wrapper cannot observe the cancellation; the next operation in the same direction resumes the pending span and reports the time since the abandoned operation began.

## Report

The terminal report renders reads and writes as stacked sub-tables (a sub-table is skipped if there were no operations of that kind). The write sub-table carries the flush count, and its Errors column aggregates write, flush, and shutdown errors so failures surfaced during flush aren't hidden; per-kind error counts appear in the JSON report, where shutdown operations are also broken out. Metrics are also exposed at `GET /io` and in the TUI on the I/O tab, Bytes sub-tab.

`Seek`, `AsyncSeek`, `BufRead`, and `AsyncBufRead` delegation is not yet instrumented.
