# hotpath - real-time Rust performance and data flow profiler
[![Latest Version](https://img.shields.io/crates/v/hotpath.svg)](https://crates.io/crates/hotpath) [![GH Actions](https://github.com/pawurb/hotpath/actions/workflows/ci.yml/badge.svg)](https://github.com/pawurb/hotpath/actions)

![hotpath TUI Example](hotpath-tui3.gif)

A lightweight Rust profiler for latency, memory, and data-flow insight. Instrument functions, channels, and streams to find bottlenecks and focus optimizations where they matter most.

In [this post](https://pawelurbanek.com/rust-optimize-performance), I explain the motivation behind the project and its inner workings.

## Features

- **Zero-cost when disabled** - fully gated by a feature flag.
- **Low-overhead** profiling for both sync and async code.
- **Live TUI dashboard** - real-time monitoring of performance data flow metrics in TUI dashboard (built with [ratatui.rs](https://ratatui.rs/)).
- **Static reports for one-off programs** - alternatively print profiling summaries without running the TUI.
- **Memory allocation tracking** - track bytes allocated and allocation counts per function.
- **Channel and stream monitoring** - instrument channels and streams to track message flow and throughput.
- **Detailed stats**: avg, total time, call count, % of total runtime, and configurable percentiles (p95, p99, etc.).
- **Background processing** for minimal profiling impact.
- **GitHub Actions integration** - configure CI to automatically benchmark your program against a base branch for each PR

## Roadmap 

- [x] latency, memory method calls tracking
- [x] channels/streams profiling
- [ ] process threads monitoring
- [ ] Tokio runtime metrics 
- [ ] Tokio tasks monitoring
- [ ] improved docs on [hotpath.rs](https://hotpath.rs)
- [ ] hosted backend integration
- [ ] interactive SSH demo 
- [ ] MCP/LLM interface

## Quick Start

> **⚠️ Note**  
> This README reflects the latest development on the `main` branch.
> For documentation matching the current release, see [crates.io](https://crates.io/crates/hotpath) - it stays in sync with the published crate.

Add to your `Cargo.toml`:

```toml
[dependencies]
hotpath = { version = "0.7", optional = true }

[features]
hotpath = ["dep:hotpath", "hotpath/hotpath"]
hotpath-alloc = ["hotpath/hotpath-alloc"]
hotpath-off = ["hotpath/hotpath-off"]
```

This config ensures that the lib has **zero** overhead unless explicitly enabled via a `hotpath` feature.

Profiling features are mutually exclusive. To ensure compatibility with `--all-features` setting, the crate defines an additional `hotpath-off` flag. This is handled automatically - you should never need to enable it manually.

## Usage

```rust
use std::time::Duration;

#[cfg_attr(feature = "hotpath", hotpath::measure)]
fn sync_function(sleep: u64) {
    std::thread::sleep(Duration::from_nanos(sleep));
}

#[cfg_attr(feature = "hotpath", hotpath::measure)]
async fn async_function(sleep: u64) {
    tokio::time::sleep(Duration::from_nanos(sleep)).await;
}

// When using with tokio, place the #[tokio::main] first
#[tokio::main]
// You can configure any percentile between 0 and 100
#[cfg_attr(feature = "hotpath", hotpath::main(percentiles = [99]))]
async fn main() {
    for i in 0..100 {
        // Measured functions will automatically send metrics
        sync_function(i);
        async_function(i * 2).await;

        // Measure code blocks with static labels
        #[cfg(feature = "hotpath")]
        hotpath::measure_block!("custom_block", {
            std::thread::sleep(Duration::from_nanos(i * 3))
        });
    }
}
```

Run your program with a `hotpath` feature:

```
cargo run --features=hotpath
```

Output:

```
[hotpath] Performance summary from basic::main (Total time: 122.13ms):
+-----------------------+-------+---------+---------+----------+---------+
| Function              | Calls | Avg     | P99     | Total    | % Total |
+-----------------------+-------+---------+---------+----------+---------+
| basic::async_function | 100   | 1.16ms  | 1.20ms  | 116.03ms | 95.01%  |
+-----------------------+-------+---------+---------+----------+---------+
| custom_block          | 100   | 17.09µs | 39.55µs | 1.71ms   | 1.40%   |
+-----------------------+-------+---------+---------+----------+---------+
| basic::sync_function  | 100   | 16.99µs | 35.42µs | 1.70ms   | 1.39%   |
+-----------------------+-------+---------+---------+----------+---------+
```

## Live Performance Metrics TUI

![hotpath TUI Example](hotpath-tui3.gif)

`hotpath` includes a live terminal-based dashboard for real-time monitoring of profiling metrics, including function performance, channel statistics, and stream throughput. This is particularly useful for long-running applications like web servers, where you want to observe performance characteristics while the application is running.

### Getting Started with TUI

**1. Install the hotpath binary with TUI support:**

```bash
cargo install hotpath --features tui
```

**2. Start your application with `--features=hotpath`:**

```bash
cargo run --features hotpath
```

**3. In a separate terminal, launch the TUI console:**

```bash
hotpath console 
```

The TUI will connect to your running application and display real-time profiling metrics with automatic refresh.

## Allocation Tracking

In addition to time-based profiling, `hotpath` can track memory allocations. This feature uses a custom global allocator from [allocation-counter crate](https://github.com/fornwall/allocation-counter) to intercept all memory allocations and provides detailed statistics about memory usage per function.

By default, allocation tracking is **cumulative**, meaning that a function's allocation count includes all allocations made by functions it calls (nested calls). Notably, it produces invalid results for recursive functions. To track only **exclusive** allocations (direct allocations made by each function, excluding nested calls), set the `HOTPATH_ALLOC_SELF=true` environment variable when running your program.

Run your program with the allocation tracking feature to print a similar report:

```
cargo run --features='hotpath,hotpath-alloc'
```

![Alloc report](hotpath-alloc-report.png)

### Profiling memory allocations for async functions

To profile memory usage of `async` functions you have to use a similar config:

```rust
#[cfg(feature = "hotpath-alloc")]
#[tokio::main(flavor = "current_thread")]
async fn main() {
    _ = inner_main().await;
}

#[cfg(not(feature = "hotpath-alloc"))]
#[tokio::main]
async fn main() {
    _ = inner_main().await;
}

#[cfg_attr(feature = "hotpath", hotpath::main)]
async fn inner_main() {
    // ...
}
```

It ensures that tokio runs in a `current_thread` runtime mode if the allocation profiling feature is enabled.

**Why this limitation exists**: The allocation tracking uses thread-local storage to track memory usage. In multi-threaded runtimes, async tasks can migrate between threads, making it impossible to accurately attribute allocations to specific function calls.

## Channel and Stream Monitoring

In addition to function profiling, `hotpath` can instrument async channels and streams to track message throughput, queue sizes, and data flow. This is particularly useful for debugging async applications and identifying bottlenecks in concurrent message-passing systems.

### Channel Monitoring

The `channel!` macro wraps channel creation to automatically track statistics:

```rust
use tokio::sync::mpsc;

#[tokio::main]
#[cfg_attr(feature = "hotpath", hotpath::main)]
async fn main() {
    // Create a channel normally
    let (tx, rx) = mpsc::channel::<String>(100);

    // Instrument it when profiling is enabled
    #[cfg(feature = "hotpath")]
    let (tx, rx) = hotpath::channel!((tx, rx));

    // Use the channel exactly as before
    tx.send("Hello".to_string()).await.unwrap();
    let msg = rx.recv().await.unwrap();
}
```

[std::sync](https://doc.rust-lang.org/stable/std/sync/mpsc/index.html) channels can be instrumented by default. Enable `tokio`, `futures`, or `crossbeam` features for [Tokio](https://github.com/tokio-rs/tokio), [futures-rs](https://github.com/rust-lang/futures-rs), and [crossbeam](https://github.com/crossbeam-rs/crossbeam) channels, respectively.

**Supported channel types:**
- [`tokio::sync::mpsc::channel`](https://docs.rs/tokio/latest/tokio/sync/mpsc/fn.channel.html)
- [`tokio::sync::mpsc::unbounded_channel`](https://docs.rs/tokio/latest/tokio/sync/mpsc/fn.unbounded_channel.html)
- [`tokio::sync::oneshot::channel`](https://docs.rs/tokio/latest/tokio/sync/oneshot/fn.channel.html)
- [`futures_channel::mpsc::channel`](https://docs.rs/futures-channel/latest/futures_channel/mpsc/fn.channel.html)
- [`futures_channel::mpsc::unbounded`](https://docs.rs/futures-channel/latest/futures_channel/mpsc/fn.unbounded.html)
- [`futures_channel::oneshot::channel`](https://docs.rs/futures-channel/latest/futures_channel/oneshot/fn.channel.html)
- [`crossbeam_channel::bounded`](https://docs.rs/crossbeam/latest/crossbeam/channel/fn.bounded.html)
- [`crossbeam_channel::unbounded`](https://docs.rs/crossbeam/latest/crossbeam/channel/fn.unbounded.html)

**Optional features:**

```rust
// Custom label for easier identification in TUI
let (tx, rx) = hotpath::channel!((tx, rx), label = "worker_queue");

// Enable message logging (requires Debug trait on message type)
let (tx, rx) = hotpath::channel!((tx, rx), log = true);
```

**Capacity parameter requirement:**

⚠️ **Important:** For `futures::channel::mpsc` bounded channels, you **must** specify the `capacity` parameter because their API doesn't expose the capacity after creation:

```rust
use futures_channel::mpsc;

// futures bounded channel - MUST specify capacity
let (tx, rx) = mpsc::channel::<String>(10);
#[cfg(feature = "hotpath")]
let (tx, rx) = hotpath::channel!((tx, rx), capacity = 10);
```

Tokio and crossbeam channels don't require this parameter because their capacity is accessible from the channel handles.


### Stream Monitoring

The `stream!` macro instruments async streams to track items yielded:

```rust
use futures::stream::{self, StreamExt};

#[tokio::main]
#[cfg_attr(feature = "hotpath", hotpath::main)]
async fn main() {
    // Create a stream
    let s = stream::iter(1..=100);

    // Instrument it when profiling is enabled
    #[cfg(feature = "hotpath")]
    let s = hotpath::stream!(s);

    // Use it normally
    let items: Vec<_> = s.collect().await;
}
```

**Optional features:**

```rust
// Custom label
let s = hotpath::stream!(s, label = "data_stream");

// Enable item logging (requires Debug trait on item type)
let s = hotpath::stream!(s, log = true);
```

### Viewing Channel and Stream Metrics in TUI

When using the live TUI dashboard, channel and stream statistics are displayed alongside function metrics. The TUI shows:

- Real-time sent/received counts for channels
- Queue sizes and queued bytes
- Items yielded for streams
- State changes (active → full → closed)
- Recent message/item logs (when logging is enabled)

See the [Live Performance Metrics TUI](#live-performance-metrics-tui) section for setup instructions.

**Environment variable:**
- `HOTPATH_LOGS_LIMIT` - Maximum number of log entries to keep per channel/stream (default: 50)

### How Channel and Stream Monitoring Works

The `channel!` macro wraps channels with lightweight proxies that transparently forward all messages while collecting real-time statistics. Each `send` and `recv` operation passes through a monitored proxy that emits updates to a background metrics collection thread.

The `stream!` macro wraps streams and tracks items as they are yielded, collecting statistics about throughput and completion.

**Zero-cost when disabled:** When the `hotpath` feature is disabled, the `#[cfg(feature = "hotpath")]` attribute ensures all instrumentation code is completely removed at compile time - there's absolutely zero runtime overhead.

**Background processing:** The first invocation of `channel!` or `stream!` automatically starts:
- A background thread for metrics collection
- An HTTP server (when `HOTPATH_HTTP_PORT` is set) exposing metrics in JSON format for the TUI

#### A note on accuracy

`hotpath` instruments proxy channels that wrap your actual channel instances. It observes messages as they pass through these proxies rather than when they are finally consumed. As a result, the displayed metrics are an approximation of real channel activity - useful for debugging and diagnosing flow issues, but not a 100% accurate source of truth for production monitoring.

Because of this proxy design, each bounded channel is effectively represented by three layers - the outer proxy, the original channel, and the inner proxy. In practice, this triples the total buffering capacity. For the same reason, it's currently not possible to measure the queue size of unbounded channels. Even with a slow consumer, the intermediate proxies will immediately absorb all incoming messages, masking true backlog behavior.

That said, since the proxy layer introduces virtually no overhead compared to direct channel usage, timing and delay metrics should remain accurate. Logged messages contents and ordering is also 100% accurate.

Current design intentionally sacrifices accuracy for the ease of integration - you can instrument channels with minimal code changes and still get meaningful visibility into their behavior.

#### There be bugs 🐛

This library has just been released. I've tested it with several apps, [big](https://x.com/_pawurb/status/1986570325341962339) and small, and it consistently produced reliable metrics. However, please note that enabling monitoring can subtly affect channel behavior in some cases. For example, using `try_send` may not return an error as expected, since the proxy layers effectively increase total capacity. I'm actively improving the library, so any feedback, issues, bug reports are appreciated.

### ChannelsGuard - Printing Statistics on Drop

In addition to the TUI, you can use `ChannelsGuard` to automatically print channel and stream statistics when your program ends (similar to function profiling output):

```rust
use tokio::sync::mpsc;

#[tokio::main]
async fn main() {
    // Create guard at the start (prints stats when dropped)
    #[cfg(feature = "hotpath")]
    let _guard = hotpath::ChannelsGuard::new();

    // Your code with instrumented channels...
    let (tx, rx) = mpsc::channel::<i32>(10);
    #[cfg(feature = "hotpath")]
    let (tx, rx) = hotpath::channel!((tx, rx), label = "task-queue");

    // ... use your channels ...

    // Statistics will be printed when _guard is dropped (at program end)
}
```

**Output example:**

```
=== Channel Statistics (runtime: 5.23s) ===

+------------------+-------------+--------+------+----------+--------+------------+
| Channel          | Type        | State  | Sent | Received | Queued | Queued Mem |
+------------------+-------------+--------+------+----------+--------+------------+
| task-queue       | bounded[10] | active | 1543 | 1543     | 0      | 0 B        |
| http-responses   | unbounded   | active | 892  | 890      | 2      | 200 B      |
| shutdown-signal  | oneshot     | closed | 1    | 1        | 0      | 0 B        |
+------------------+-------------+--------+------+----------+--------+------------+
```

**Customize output format:**

```rust
#[cfg(feature = "hotpath")]
let _guard = hotpath::ChannelsGuardBuilder::new()
    .format(hotpath::Format::Json)
    .build();
```

## How It Works

1. `#[cfg_attr(feature = "hotpath", hotpath::main)]` - Macro that initializes the background measurement processing
2. `#[cfg_attr(feature = "hotpath", hotpath::measure)]` - Macro that wraps functions with profiling code
3. **Background thread** - Measurements are sent to a dedicated worker thread via bounded channel
4. **Statistics aggregation** - Worker thread maintains running statistics for each function/code block
5. **Automatic reporting** - Performance summary displayed when the program exits

## API

### Macros

#### `#[hotpath::main]`

Attribute macro that initializes the background measurement processing when applied. Supports parameters:
- `percentiles = [50, 95, 99]` - Custom percentiles to display
- `format = "json"` - Output format ("table", "json", "json-pretty")
- `limit = 20` - Maximum number of functions to display (default: 15, 0 = show all)
- `timeout = 5000` - Optional timeout in milliseconds. If specified, the program will print the report and exit after the timeout (useful for profiling long-running programs like HTTP servers)

#### `#[hotpath::measure]`

An opt-in attribute macro that instruments functions to send timing measurements to the background processor.

#### `#[hotpath::measure_all]`

An attribute macro that applies `#[measure]` to all functions in a `mod` or `impl` block. Useful for bulk instrumentation without annotating each function individually. Can be used on:
- **Inline module declarations** - Instruments all functions within the module
- **Impl blocks** - Instruments all methods in the implementation

Example:

```rust
// Measure all methods in an impl block
#[cfg_attr(feature = "hotpath", hotpath::measure_all)]
impl Calculator {
    fn add(&self, a: u64, b: u64) -> u64 { a + b }
    fn multiply(&self, a: u64, b: u64) -> u64 { a * b }
    async fn async_compute(&self) -> u64 { /* ... */ }
}

// Measure all functions in a module
#[cfg_attr(feature = "hotpath", hotpath::measure_all)]
mod math_operations {
    pub fn complex_calculation(x: f64) -> f64 { /* ... */ }
    pub async fn fetch_data() -> Vec<u8> { /* ... */ }
}
```

> **Note:** Once Rust stabilizes [`#![feature(proc_macro_hygiene)]`](https://doc.rust-lang.org/beta/unstable-book/language-features/proc-macro-hygiene.html?highlight=proc_macro_hygiene#proc_macro_hygiene) and [`#![feature(custom_inner_attributes)]`](https://doc.rust-lang.org/beta/unstable-book/language-features/custom-inner-attributes.html), it will be possible to use `#![measure_all]` as an inner attribute directly inside module files (e.g., at the top of `math_operations.rs`) to automatically instrument all functions in that module.

#### `#[hotpath::skip]`

A marker attribute that excludes specific functions from instrumentation when used within a module or impl block annotated with `#[measure_all]`. The function executes normally but doesn't send measurements to the profiling system.

Example:

```rust
#[cfg_attr(feature = "hotpath", hotpath::measure_all)]
mod operations {
    pub fn important_function() { /* ... */ } // Measured

    #[cfg_attr(feature = "hotpath", hotpath::skip)]
    pub fn not_so_important_function() { /* ... */ } // NOT measured
}
```

#### `hotpath::measure_block!(label, expr)`

Macro that measures the execution time of a code block with a static string label.

#### `hotpath::channel!(expr)`

Macro that instruments channels to track message flow statistics. Wraps channel creation with monitoring code that tracks sent/received counts, queue size, and channel state.

**Supported patterns:**
- `hotpath::channel!((tx, rx))` - Basic instrumentation
- `hotpath::channel!((tx, rx), label = "name")` - With custom label
- `hotpath::channel!((tx, rx), log = true)` - With message logging (requires Debug trait)
- `hotpath::channel!((tx, rx), label = "name", log = true)` - Both options combined

**Supported channel types:** `tokio::sync::mpsc`, `tokio::sync::oneshot`, `futures_channel::mpsc`, `crossbeam_channel`

#### `hotpath::stream!(expr)`

Macro that instruments streams to track items yielded. Wraps stream creation with monitoring code that tracks yield count and stream state.

**Supported patterns:**
- `hotpath::stream!(s)` - Basic instrumentation
- `hotpath::stream!(s, label = "name")` - With custom label
- `hotpath::stream!(s, log = true)` - With item logging (requires Debug trait)
- `hotpath::stream!(s, label = "name", log = true)` - Both options combined

### GuardBuilder API (Function Profiling)

`hotpath::GuardBuilder::new(caller_name)` - Create a new builder with the specified caller name

**Configuration methods:**
- `.percentiles(&[u8])` - Set custom percentiles to display (default: [95])
- `.format(Format)` - Set output format (Table, Json, JsonPretty)
- `.limit(usize)` - Set maximum number of functions to display (default: 15, 0 = show all)
- `.reporter(Box<dyn Reporter>)` - Set custom reporter (overrides format)
- `.build()` - Build and return the HotPath guard
- `.build_with_timeout(Duration)` - Build guard that automatically drops after duration and exits the program (useful for profiling long-running programs like HTTP servers)

### ChannelsGuard API (Channel Monitoring)

`hotpath::ChannelsGuard::new()` - Create a guard that prints channel statistics when dropped

`hotpath::ChannelsGuardBuilder::new()` - Create a builder for customizing channel statistics output

**Configuration methods:**
- `.format(Format)` - Set output format (Table, Json, JsonPretty)
- `.build()` - Build and return the ChannelsGuard

**Example:**
```rust
#[cfg(feature = "hotpath")]
let _guard = hotpath::ChannelsGuardBuilder::new()
    .format(hotpath::Format::JsonPretty)
    .build();
```

### StreamsGuard API (Stream Monitoring)

`hotpath::StreamsGuard::new()` - Create a guard that prints stream statistics when dropped

`hotpath::StreamsGuardBuilder::new()` - Create a builder for customizing stream statistics output

**Configuration methods:**
- `.format(Format)` - Set output format (Table, Json, JsonPretty)
- `.build()` - Build and return the StreamsGuard

**Example:**
```rust
#[cfg(feature = "hotpath")]
let _guard = hotpath::StreamsGuardBuilder::new()
    .format(hotpath::Format::Table)
    .build();
```

**Example:**
```rust
let _guard = hotpath::GuardBuilder::new("main")
    .percentiles(&[50, 90, 95, 99])
    .limit(20)
    .format(hotpath::Format::JsonPretty)
    .build();
```

**Timed profiling example**

```rust
use std::time::Duration;

#[cfg_attr(feature = "hotpath", hotpath::measure)]
fn work_function() {
    std::thread::sleep(Duration::from_millis(10));
}

fn main() {
    // Profile for 1 second, then generate report and exit
    #[cfg(feature = "hotpath")]
    hotpath::GuardBuilder::new("timed_benchmark")
        .build_with_timeout(Duration::from_secs(1));

    loop {
        work_function();
    }
}
```

## Usage Patterns

### Using `hotpath::main` macro vs `GuardBuilder` API

The `#[hotpath::main]` macro is convenient for most use cases, but the `GuardBuilder` API provides more control over when profiling starts and stops.

Key differences:

- **`#[hotpath::main]`** - Automatic initialization and cleanup, report printed at program exit
- **`let _guard = GuardBuilder::new("name").build()`** - Manual control, report printed when guard is dropped, so you can fine-tune the measured scope.

Only one hotpath guard may be alive at a time, regardless of whether it was created by the `main` macro or by the builder API. If a second guard is created, the library will panic.

#### Using `GuardBuilder` for more control

```rust
use std::time::Duration;

#[cfg_attr(feature = "hotpath", hotpath::measure)]
fn example_function() {
    std::thread::sleep(Duration::from_millis(10));
}

fn main() {
    #[cfg(feature = "hotpath")]
    let _guard = hotpath::GuardBuilder::new("my_program")
        .percentiles(&[50, 95, 99])
        .format(hotpath::Format::Table)
        .build();

    example_function();

    // This will print the report.
    #[cfg(feature = "hotpath")]
    drop(_guard);

    // Immediate exit (no drops); `#[hotpath::main]` wouldn't print.
    std::process::exit(1);
}
```

#### Using in unit tests

In unit tests you can profile each individual test case:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sync_function() {
        #[cfg(feature = "hotpath")]
        let _hotpath = hotpath::GuardBuilder::new("test_sync_function")
            .percentiles(&[50, 90, 95])
            .format(hotpath::Format::Table)
            .build();
        sync_function();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn test_async_function() {
        #[cfg(feature = "hotpath")]
        let _hotpath = hotpath::GuardBuilder::new("test_async_function")
            .percentiles(&[50, 90, 95])
            .format(hotpath::Format::Table)
            .build();

        async_function().await;
    }
}
```

Run tests with profiling enabled:

```bash
cargo test --features hotpath -- --test-threads=1
```

Note: Use `--test-threads=1` to ensure tests run sequentially, as only one hotpath guard can be active at a time.

### Percentiles Support

By default, `hotpath` displays P95 percentile in the performance summary. You can customize which percentiles to display using the `percentiles` parameter:

```rust
#[tokio::main]
#[cfg_attr(feature = "hotpath", hotpath::main(percentiles = [50, 75, 90, 95, 99]))]
async fn main() {
    // Your code here
}
```

For multiple measurements of the same function or code block, percentiles help identify performance distribution patterns. You can use percentile 0 to display min value and 100 to display max.

### Output Formats

By default, `hotpath` displays results in a human-readable table format. You can also output results in JSON format for programmatic processing:

```rust
#[tokio::main]
#[cfg_attr(feature = "hotpath", hotpath::main(format = "json-pretty"))]
async fn main() {
    // Your code here
}
```

Supported format options:
- `"table"` (default) - Human-readable table format
- `"json"` - Compact, oneline JSON format
- `"json-pretty"` - Pretty-printed JSON format

Example JSON output:

```json
{
  "hotpath_profiling_mode": "timing",
  "output": {
    "basic::async_function": {
      "calls": "100",
      "avg": "1.16ms",
      "p95": "1.26ms",
      "total": "116.41ms",
      "percent_total": "96.18%"
    },
    "basic::sync_function": {
      "calls": "100",
      "avg": "23.10µs",
      "p95": "37.89µs",
      "total": "2.31ms",
      "percent_total": "1.87%"
    }
  }
}
```

You can combine multiple parameters:

```rust
#[cfg_attr(feature = "hotpath", hotpath::main(percentiles = [50, 90, 99], format = "json", limit = 10, timeout = 30000))]
```

## Custom Reporters

You can implement your own reporting to control how profiling results are handled. This allows you to plug `hotpath` into existing tools like loggers, CI pipelines, or monitoring systems.

For complete working examples, see:
- [`examples/csv_file_reporter.rs`](crates/test-tokio-async/examples/csv_file_reporter.rs) - Save metrics to CSV file
- [`examples/json_file_reporter.rs`](crates/test-tokio-async/examples/json_file_reporter.rs) - Save metrics to JSON file
- [`examples/tracing_reporter.rs`](crates/test-tokio-async/examples/tracing_reporter.rs) - Log metrics using the tracing crate

## Benchmarking

Measure overhead of profiling 10k method calls with [hyperfine](https://github.com/sharkdp/hyperfine):

Timing:
```
cargo build --example benchmark --features hotpath --release
hyperfine --warmup 3 './target/release/examples/benchmark'
```

Allocations:
```
cargo build --example benchmark --features='hotpath,hotpath-alloc' --release
hyperfine --warmup 3 './target/release/examples/benchmark'
```
