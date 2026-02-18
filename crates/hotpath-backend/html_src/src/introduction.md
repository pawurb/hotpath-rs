# Rust Performance & Memory Profiler

<div class="hero-row">
  <img src="{{#asset-hash images/hotpath-ferris.webp}}" alt="hotpath-rs Rust profiler mascot Ferris the crab" class="ferris-img-hero">
  <div class="ssh-demo-container">
    <p class="ssh-demo-label">Try the TUI demo via SSH - no installation required:</p>
    <div class="terminal-shell">
      <span class="terminal-prompt">$</span>
      <span class="terminal-command">ssh demo.hotpath.rs</span>
    </div>
  </div>
</div>

[hotpath-rs](https://github.com/pawurb/hotpath-rs) is a Rust performance profiler that instruments functions, channels, futures, and streams. It helps you find runtime bottlenecks and optimize where it matters most, with detailed metrics for time, memory, and async data flow monitoring.
<div style="clear: both;"></div>

<div class="trusted-by">
  <p class="trusted-by-tagline">Trusted by dozens of open-source projects, including:</p>
  <div class="trusted-by-grid">
    <a href="https://github.com/apache/opendal" target="_blank" class="trusted-by-project">
      <span class="trusted-by-name">apache/opendal</span>
      <img src="{{#asset-hash images/stars-apache-opendal.svg}}" alt="opendal GitHub stars">
    </a>
    <a href="https://github.com/apache/horaedb" target="_blank" class="trusted-by-project">
      <span class="trusted-by-name">apache/horaedb</span>
      <img src="{{#asset-hash images/stars-apache-horaedb.svg}}" alt="horaedb GitHub stars">
    </a>
    <a href="https://github.com/marc2332/freya" target="_blank" class="trusted-by-project">
      <span class="trusted-by-name">marc2332/freya</span>
      <img src="{{#asset-hash images/stars-marc2332-freya.svg}}" alt="freya GitHub stars">
    </a>
    <a href="https://github.com/tqwewe/kameo" target="_blank" class="trusted-by-project">
      <span class="trusted-by-name">tqwewe/kameo</span>
      <img src="{{#asset-hash images/stars-tqwewe-kameo.svg}}" alt="kameo GitHub stars">
    </a>
    <a href="https://github.com/tryandromeda/andromeda" target="_blank" class="trusted-by-project">
      <span class="trusted-by-name">tryandromeda/andromeda</span>
      <img src="{{#asset-hash images/stars-tryandromeda-andromeda.svg}}" alt="andromeda GitHub stars">
    </a>
    <a href="https://github.com/nyudenkov/pysentry" target="_blank" class="trusted-by-project">
      <span class="trusted-by-name">nyudenkov/pysentry</span>
      <img src="{{#asset-hash images/stars-nyudenkov-pysentry.svg}}" alt="pysentry GitHub stars">
    </a>
  </div>
</div>

You can use it to produce one-off performance (timing or memory) reports:

<img loading="lazy" src="{{#asset-hash images/hotpath-alloc-report.png}}" alt="hotpath-rs memory allocation profiling report showing per-function byte counts">

or use the live TUI dashboard to monitor real-time performance and data flow metrics with debug info:

<video loading="lazy" width="100%" loop muted playsinline controls poster="{{#asset-hash images/hotpath-live-dashboard-poster.jpg}}">
  <source src="{{#asset-hash videos/hotpath-live-dashboard.mp4}}" type="video/mp4">
</video>

## Features

- **Zero-cost when disabled** - fully gated by a feature flag.
- **Low-overhead** profiling for both sync and async code.
- **Live TUI dashboard** - live TUI dashboard for performance + async data-flow metrics. (built with <a href="https://ratatui.rs/" target="_blank">ratatui.rs</a>).
- **Static reports for one-off programs** - alternatively print profiling summaries without running the TUI.
- **Memory allocation tracking** - track bytes allocated and allocation counts per function.
- **Channel and stream monitoring** - instrument channels and streams to track message flow and throughput.
- **Futures instrumentation** - monitor any async piece of code to track poll counts, lifecycle and resolved values.
- **Detailed stats**: avg, total time, call count, % of total runtime, and configurable percentiles (p95, p99, etc.).
- **Tokio runtime monitoring** - track worker thread utilization, task scheduling, and queue depths.
- **MCP server for AI agents** - built-in <a href="https://modelcontextprotocol.io/" target="_blank">Model Context Protocol</a> server that lets LLMs query profiling data in real-time.
- **GitHub Actions integration** - configure CI to automatically benchmark your program against a base branch for each PR.

## Quick demo

Other than the SSH demo an easy way to quickly try the TUI is to run it in **auto-instrumentation mode**. The TUI process profiles itself and displays its own performance metrics in real time.

First, install `hotpath` CLI with auto-instrumentation enabled:

```bash
cargo install hotpath --features='tui,hotpath,hotpath-alloc'
```

Then launch the TUI:

```bash
hotpath
```

and you'll see timing, memory and other metrics.

Make sure to reinstall it without the auto-profiling features so that you can also observe metrics of other programs!

```bash
cargo install hotpath --features='tui'
```

## Getting Started

### Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
hotpath = "0.11"

[features]
hotpath = ["hotpath/hotpath"]
hotpath-alloc = ["hotpath/hotpath-alloc"]
```

This config ensures that the lib has no compile time or runtime overhead unless explicitly enabled via a `hotpath` feature. All the lib dependencies are optional (i.e. not compiled) and all macros are noop unless profiling is enabled.

### Basic setup

You'll need only `#[hotpath::main]` and `#[hotpath::measure]` macros to get started:

```rust
#[hotpath::measure]
fn sync_function(sleep: u64) {
    std::thread::sleep(Duration::from_nanos(sleep));
    let vec1 = vec![1, 2, 3];
    std::hint::black_box(&vec1); // force mem allocation
}

#[hotpath::measure]
async fn async_function(sleep: u64) {
    tokio::time::sleep(Duration::from_nanos(sleep)).await;
}

// When using with tokio, place the #[tokio::main] first
#[tokio::main]
#[hotpath::main]
async fn main() {
    for i in 0..10000 {
        sync_function(i);
        async_function(i * 2).await;

        hotpath::measure_block!("custom_block", {
            std::thread::sleep(Duration::from_nanos(i * 3))
        });
    }
}
```

Now, run your program with `hotpath` (and optionally `hotpath-alloc` features):

```bash
cargo run --features='hotpath,hotpath-alloc'
```

On exit it will print a report with timings, memory allocations and thread usage metrics:

```
[hotpath] 1.20s | timing, alloc, threads

timing - Function execution time metrics.
+------------------------------+-------+----------+----------+----------+---------+
| Function                     | Calls | Avg      | P95      | Total    | % Total |
+------------------------------+-------+----------+----------+----------+---------+
| docs_example::main           | 1     | 1.20 s   | 1.20 s   | 1.20 s   | 100.00% |
+------------------------------+-------+----------+----------+----------+---------+
| docs_example::async_function | 1000  | 1.15 ms  | 1.20 ms  | 1.15 s   | 96.10%  |
+------------------------------+-------+----------+----------+----------+---------+
| custom_block                 | 1000  | 18.13 µs | 31.71 µs | 18.13 ms | 1.51%   |
+------------------------------+-------+----------+----------+----------+---------+
| docs_example::sync_function  | 1000  | 16.58 µs | 27.63 µs | 16.58 ms | 1.38%   |
+------------------------------+-------+----------+----------+----------+---------+

alloc - Cumulative allocations during each function call (including nested calls).
+------------------------------+-------+---------+---------+---------+---------+
| Function                     | Calls | Avg     | P95     | Total   | % Total |
+------------------------------+-------+---------+---------+---------+---------+
| docs_example::main           | 1     | 63.0 KB | 63.1 KB | 63.0 KB | 100.00% |
+------------------------------+-------+---------+---------+---------+---------+
| docs_example::sync_function  | 1000  | 12 B    | 12 B    | 11.7 KB | 18.58%  |
+------------------------------+-------+---------+---------+---------+---------+
| custom_block                 | 1000  | 0 B     | 0 B     | 0 B     | 0.00%   |
+------------------------------+-------+---------+---------+---------+---------+
| docs_example::async_function | 1000  | 0 B     | 0 B     | 0 B     | 0.00%   |
+------------------------------+-------+---------+---------+---------+---------+

threads - Thread CPU and memory statistics. (RSS: 7.8 MB, Alloc: 2.1 MB, Dealloc: 304.3 KB, Diff: 1.8 MB, 5/10)
+--------------+----------+------+------+----------+---------+-----------+----------+----------+----------+
| Thread       | Status   | CPU% | Max% | CPU User | CPU Sys | CPU Total | Alloc    | Dealloc  | Diff     |
+--------------+----------+------+------+----------+---------+-----------+----------+----------+----------+
| hp-functions | Sleeping | 1.8% | 1.8% | 0.018s   | 0.001s  | 0.019s    | 1.8 MB   | 291.3 KB | 1.5 MB   |
+--------------+----------+------+------+----------+---------+-----------+----------+----------+----------+
| main         | Sleeping | 6.3% | 6.3% | 0.123s   | 0.070s  | 0.193s    | 367.8 KB | 9.9 KB   | 357.9 KB |
+--------------+----------+------+------+----------+---------+-----------+----------+----------+----------+
| hp-threads   | Running  | 0.0% | 0.0% | 0.000s   | 0.001s  | 0.001s    | 10.3 KB  | 3.0 KB   | 7.3 KB   |
+--------------+----------+------+------+----------+---------+-----------+----------+----------+----------+
| hp-server    | Sleeping | 0.0% | 0.0% | 0.000s   | 0.001s  | 0.001s    | 1.8 KB   | 56 B     | 1.7 KB   |
+--------------+----------+------+------+----------+---------+-----------+----------+----------+----------+
| thread_5     | Sleeping | -    | -    | 0.000s   | 0.000s  | 0.000s    | 640 B    | 24 B     | 616 B    |
+--------------+----------+------+------+----------+---------+-----------+----------+----------+----------+
```

## Learn more

Explore the docs for customization options and advanced profiling features.

- [Sampling Comparison](./sampling_comparison.html) - when to use `hotpath` vs CPU sampling profilers
- [Profiling modes](./profiling_modes.html) - static reports vs live TUI dashboard
- [Functions](./functions.html) - measure execution time and memory allocations
- [Futures](./futures.html) - monitor async code, poll counts, and resolved values
- [Channels](./channels.html) - track messages flow and throughput
- [Streams](./streams.html) - instrument async streams
- [Threads](./threads.html) - monitor threads usage
- [Tokio Runtime](./tokio_runtime.html) - monitor Tokio runtime worker stats and task scheduling
- [MCP Server](./mcp.html) - LLM integration via Model Context Protocol
- [GitHub CI](./github_ci.html) - automated benchmarking and regression detection in CI
