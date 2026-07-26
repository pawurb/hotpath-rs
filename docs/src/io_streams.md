# Monitor File, Socket and Async I/O in Rust

`hotpath` instruments byte-level I/O to surface slow reads and writes - files, sockets, TLS streams, TCP connections like Redis, compression streams, and any custom I/O type. For every wrapped value it tracks, per operation kind (read, write, flush, shutdown):

```bash
+-------+-------+----------+-----------+-----------+-----------+--------+--------+
| Io    | Reads | Bytes    | Rate      | Avg       | P95       | Total  | Errors |
+-------+-------+----------+-----------+-----------+-----------+--------+--------+
| redis | 33000 | 322.3 KB | 75.2 KB/s | 129.85 µs | 157.57 µs | 4.28 s | 0      |
+-------+-------+----------+-----------+-----------+-----------+--------+--------+

+-------+--------+----------+-----------+---------+---------+----------+---------+--------+
| Io    | Writes | Bytes    | Rate      | Avg     | P95     | Total    | Flushes | Errors |
+-------+--------+----------+-----------+---------+---------+----------+---------+--------+
| redis | 33000  | 945.3 KB | 15.7 MB/s | 1.78 µs | 3.21 µs | 58.76 ms | 0       | 0      |
+-------+--------+----------+-----------+---------+---------+----------+---------+--------+
```

- **Operation count** - completed operations
- **Bytes processed** - total bytes read or written
- **Rate** - per-operation transfer speed: bytes divided by summed in-flight operation time, waiting included
- **Duration** - average and configured percentiles. Synchronous operations measure the full method call; async operations measure from the first poll to `Ready`, so reported durations include async waiting time (e.g. waiting for a socket to become readable), not only the final poll execution.
- **Errors** - failed operations. Retryable conditions (`WouldBlock`, `Interrupted`) are not counted as errors and produce no operation.

The `io!` macro is noop unless the `hotpath` feature is activated.

## io! macro

Wrap any value implementing `std::io::Read`, `std::io::Write`, `tokio::io::AsyncRead`, or `tokio::io::AsyncWrite` (async traits require the `tokio` feature). The wrapper delegates every operation to the wrapped value, so it is a drop-in replacement.

Profiling file read operation:
```rust
use std::io::Read;

let mut file = hotpath::io!(std::fs::File::open("data.bin")?, label = "data-file");
let mut buf = Vec::new();
file.read_to_end(&mut buf)?;
```

See the runnable [basic_io_sync](https://github.com/pawurb/hotpath-rs/blob/main/crates/test-io/examples/basic_io_sync.rs) and [basic_io_async](https://github.com/pawurb/hotpath-rs/blob/main/crates/test-io/examples/basic_io_async.rs) examples.

Async I/O works the same way.

Profiling Redis TCP connection:
```rust
use tokio::io::{AsyncReadExt, AsyncWriteExt};

let stream = tokio::net::TcpStream::connect("127.0.0.1:6379").await?;
let mut stream = hotpath::io!(stream, label = "redis");
stream.write_all(b"PING\r\n").await?;

let mut buf = [0u8; 7];
stream.read_exact(&mut buf).await?; // +PONG\r\n
```

See the runnable [basic_redis_io](https://github.com/pawurb/hotpath-rs/blob/main/crates/test-io/examples/basic_redis_io.rs) example.

The `label` parameter is optional; without it the wrapper is identified by `file:line`.

Report entries are keyed by creation site and concrete type: wrappers created repeatedly at one `io!` call - for example per accepted connection in a server loop - accumulate into a single entry, so profiler memory stays bounded by the number of `io!` call sites rather than the number of values ever wrapped.

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

Entries are keyed by creation site: all wrapper instances created at one `io!` call (e.g. per accepted connection in a server) accumulate into a single row, and the `Inst` column shows how many instances that row aggregates. Pass `iter = true` to give every instance its own row instead (displayed as `label`, `label-2`, `label-3`, ...) with individual rates and byte counts - profiler state then grows with the number of instances ever created, so prefer the default aggregation for call sites with unbounded instance churn.

`Rate` is per-operation transfer speed - bytes divided by summed in-flight operation time (waiting included, so on request/response traffic it reads as application-observed speed rather than wire speed). For a row aggregating concurrent instances the rate stays duration-weighted per operation, not the call site's aggregate bandwidth; `Rate * Inst` bounds the aggregate from above when all instances operate concurrently. Under time sampling the rate is computed from timed operations only, and shows `-` in count-only mode.

`Seek`, `AsyncSeek`, `BufRead`, and `AsyncBufRead` delegation is not yet instrumented.
