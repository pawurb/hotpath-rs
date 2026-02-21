# Tokio runtime performance monitoring: worker stats and task metrics

`hotpath` can monitor Tokio runtime performance by polling [`tokio::runtime::RuntimeMetrics`](https://docs.rs/tokio/latest/tokio/runtime/struct.RuntimeMetrics.html) on a dedicated background thread. This gives you visibility into worker thread utilization, task scheduling metrics, and queue depths without modifying your async code.

## Setup

Enable the `tokio` feature:

```toml
[dependencies]
hotpath = { version = "0.10", features = ["tokio"] }
```

Then call `tokio_runtime!()` inside your async main:

```rust
#[tokio::main]
#[hotpath::main]
async fn main() {
    hotpath::tokio_runtime!();

    // ...
}
```

You can also pass an explicit handle if you're not inside a Tokio context:

```rust
let handle = tokio::runtime::Handle::current();
hotpath::tokio_runtime!(&handle);
```

The macro spawns a dedicated `hp-runtime` background thread that periodically samples runtime metrics and stores snapshots for the TUI and HTTP API.

## Metrics collected

### Always available (stable Tokio API)

Global metrics:
- **Workers** - number of runtime worker threads
- **Alive tasks** - number of currently alive tasks
- **Global queue depth** - tasks waiting in the global injection queue

Per-worker metrics:
- **Park count** - how many times the worker has been parked (idle)
- **Busy duration** - cumulative time the worker has spent executing tasks

### With `tokio_unstable` (additional metrics)

Per-worker:
- **Poll count** - total number of task polls
- **Steal count** - tasks stolen from other workers
- **Steal operations** - number of steal attempts
- **Overflow count** - times the local queue overflowed to the global queue
- **Local queue depth** - current number of tasks in the worker's local queue
- **Mean poll time** - average time spent per poll

Global:
- **Blocking threads** - total and idle blocking threads
- **Blocking queue depth** - tasks waiting for a blocking thread
- **Spawned tasks count** - total tasks spawned since runtime start
- **Remote schedule count** - tasks scheduled from outside the runtime
- **IO driver FD count** - registered and deregistered file descriptors
- **IO ready count** - I/O readiness events

To enable unstable metrics, build with:

```bash
RUSTFLAGS="--cfg tokio_unstable" cargo run --features='hotpath'
```

## TUI view

The TUI dashboard includes a dedicated Tokio Runtime panel that displays per-worker stats in a table alongside a summary line with global metrics.

## Environment variables

- `HOTPATH_TOKIO_RUNTIME_INTERVAL` - sampling interval in milliseconds (default: 5000)
