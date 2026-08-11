# A simple Rust profiler that shows exactly why your code is slow

<h2 class="hero-subtitle">Profile CPU, memory, async execution, SQL and HTTP calls, I/O streams, lock contention and channels from a single tool.</h2>

<div class="hero-badges">
  <a href="https://github.com/pawurb/hotpath-rs" target="_blank"><img src="{{#asset-hash images/stars-pawurb-hotpath-rs.svg}}" alt="GitHub Stars"></a>
  <a href="https://crates.io/crates/hotpath" target="_blank"><img src="https://img.shields.io/crates/d/hotpath?cacheSeconds=86400" alt="crates.io downloads"></a>
</div>

<div class="hero-row">
  <img src="{{#asset-hash images/hotpath-ferris.webp}}" alt="hotpath-rs Rust profiler mascot Ferris the crab" class="ferris-img-hero">
  <div class="ssh-demo-container">
    <p class="ssh-demo-label">Try the TUI demo via SSH - no installation required:</p>
    <div class="terminal-shell">
      <span class="terminal-prompt">$</span>
      <span class="terminal-command">ssh demo.hotpath.rs</span>
    </div>
    <p class="ssh-demo-label">Or let your own AI agent configure profiling in a repo:</p>
    <div class="terminal-shell terminal-shell-multi">
      <div class="terminal-tabs">
        <button class="terminal-tab active" data-agent="claude" onclick="hotpathInitAgent('claude')">Claude</button>
        <button class="terminal-tab" data-agent="codex&nbsp;" onclick="hotpathInitAgent('codex&nbsp;')">Codex</button>
      </div>
      <div class="terminal-line">
        <span class="terminal-prompt">$</span>
        <span class="terminal-command">cargo install hotpath</span>
      </div>
      <div class="terminal-line">
        <span class="terminal-prompt">$</span>
        <span class="terminal-command">hotpath init --agent <span id="init-agent-name">claude</span></span>
      </div>
    </div>
  </div>
</div>

<script>
function hotpathInitAgent(agent) {
  document.getElementById('init-agent-name').textContent = agent;
  document.querySelectorAll('.terminal-tab').forEach(function (tab) {
    tab.classList.toggle('active', tab.dataset.agent === agent);
  });
}
</script>

[hotpath-rs](https://github.com/pawurb/hotpath-rs) is an easy-to-configure Rust performance profiling toolkit that shows exactly where your code spends time, burns CPU, and allocates memory. 

It helps you distinguish between functions that are slow because they wait on I/O and those that are CPU-intensive. Instrument functions, channels, futures, streams, SQL queries, HTTP calls, and byte-level I/O to find bottlenecks and focus optimizations where they matter most. Get actionable insights into time, memory, and async data flow with minimal setup.

<div style="clear: both;"></div>

<div class="trusted-by">
  <p class="trusted-by-tagline">Used by <a href="/adoption">{{#adoption_count}} open-source projects</a>, including:</p>
  <div class="trusted-by-grid">
    <a href="https://github.com/rustfs/rustfs" target="_blank" class="trusted-by-project">
      <span class="trusted-by-name">rustfs/rustfs</span>
      <img src="{{#asset-hash images/stars-rustfs-rustfs.svg}}" alt="rustfs GitHub stars">
    </a>
    <a href="https://github.com/apache/opendal" target="_blank" class="trusted-by-project">
      <span class="trusted-by-name">apache/opendal</span>
      <img src="{{#asset-hash images/stars-apache-opendal.svg}}" alt="opendal GitHub stars">
    </a>
    <a href="https://github.com/maplibre/martin" target="_blank" class="trusted-by-project">
      <span class="trusted-by-name">maplibre/martin</span>
      <img src="{{#asset-hash images/stars-maplibre-martin.svg}}" alt="martin GitHub stars">
    </a>
    <a href="https://github.com/marc2332/freya" target="_blank" class="trusted-by-project">
      <span class="trusted-by-name">marc2332/freya</span>
      <img src="{{#asset-hash images/stars-marc2332-freya.svg}}" alt="freya GitHub stars">
    </a>
    <a href="https://github.com/parseablehq/parseable" target="_blank" class="trusted-by-project">
      <span class="trusted-by-name">parseablehq/parseable</span>
      <img src="{{#asset-hash images/stars-parseablehq-parseable.svg}}" alt="parseable GitHub stars">
    </a>
    <a href="https://github.com/tqwewe/kameo" target="_blank" class="trusted-by-project">
      <span class="trusted-by-name">tqwewe/kameo</span>
      <img src="{{#asset-hash images/stars-tqwewe-kameo.svg}}" alt="kameo GitHub stars">
    </a>
  </div>
</div>

You can use it to produce one-off performance (timing, memory or CPU) reports:

<img loading="lazy" src="{{#asset-hash images/hotpath-alloc-report.png}}" alt="hotpath-rs memory allocation profiling report showing per-function byte counts">

inspect throughput and latency of network, file or compression I/O streams:

<img loading="lazy" src="{{#asset-hash images/io_metrics.png}}" alt="hotpath-rs I/O profiling report showing per-stream read counts, bytes, transfer rate, average and P95 latency">

analyze SQL/HTTP calls performance with automatic source function attribution:

<img loading="lazy" src="{{#asset-hash images/sql_metrics.png}}" alt="hotpath-rs SQL query profiling report showing per-query call counts, source function attribution, average and P95 execution time">

monitor throughput, performance and max queue depth of instrumented channels:

<img loading="lazy" src="{{#asset-hash images/channel_metrics.png}}" alt="hotpath-rs channel profiling report showing throughput, send-to-receive latency and max queue depth per channel">

or use the live TUI dashboard to monitor real-time performance and async data flow metrics with debug info:

<video loading="lazy" width="100%" loop muted playsinline controls poster="{{#asset-hash images/hotpath-live-dashboard-poster.jpg}}">
  <source src="{{#asset-hash videos/hotpath-live-dashboard.mp4}}" type="video/mp4">
</video>

## Features

- **Time, CPU & memory profiling** - identify expensive functions, allocation hotspots, and investigate memory leaks.
- **Async observability** - futures, channels and streams.
- **I/O monitoring** - bytes, throughput, latency of any sync or async IO stream like files, TCP, or compression.
- **SQL query profiling** - query performance metrics for sqlx and Diesel.
- **HTTP calls profiling** - per-endpoint latency and error metrics for reqwest.
- **Concurrency metrics** - Mutex/RwLock wait time and contention.
- **Tokio runtime monitoring** - workers, scheduling and queues.
- **Live TUI dashboard & static reports** - real-time or one-off analysis.
- **CI regression detection** - benchmark every PR automatically.
- **MCP server for AI agents** - query profiling data in real time.
- **Zero cost when disabled** - fully feature-gated.

<div class="waitlist-card" id="waitlist">
  <h2 class="waitlist-card-title">Every Rust PR gets a performance review.</h2>
  <p>Catch regressions in memory, SQL queries, HTTP calls and concurrency bottlenecks before they reach production. Iterate on reproducible signals, not CI noise.</p>
  <img src="{{#asset-hash images/hotpath-team-poc.webp}}" class="waitlist-card-image" alt="Hotpath Team commit timeline comparing duration, memory, HTTP and SQL metrics across commits, flagging a PR that introduced 171 new SQL calls" loading="lazy" width="1672" height="941">
  <p class="waitlist-cta-note">Launching soon • Early access invitations will be sent to waitlist members first.</p>
  <div class="waitlist-cta-row">
    <a href="/auth/github/login" class="waitlist-cta"><svg class="waitlist-cta-icon" viewBox="0 0 16 16" width="18" height="18" aria-hidden="true" fill="currentColor"><path d="M8 0C3.58 0 0 3.58 0 8c0 3.54 2.29 6.53 5.47 7.59.4.07.55-.17.55-.38 0-.19-.01-.82-.01-1.49-2.01.37-2.53-.49-2.69-.94-.09-.23-.48-.94-.82-1.13-.28-.15-.68-.52-.01-.53.63-.01 1.08.58 1.23.82.72 1.21 1.87.87 2.33.66.07-.52.28-.87.51-1.07-1.78-.2-3.64-.89-3.64-3.95 0-.87.31-1.59.82-2.15-.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82.64-.18 1.32-.27 2-.27.68 0 1.36.09 2 .27 1.53-1.04 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56.82 1.27.82 2.15 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48 0 1.07-.01 1.93-.01 2.2 0 .21.15.46.55.38A8.013 8.013 0 0016 8c0-4.42-3.58-8-8-8z"></path></svg>Join waitlist with GitHub</a>
  </div>
</div>
<p class="waitlist-building-note">Building in public. Follow development progress on X: <a href="https://x.com/pawelurbanekcom">@pawelurbanekcom</a></p>

## Getting Started

### AI Setup (Recommended)

The quickest way to set up hotpath is to let your own AI coding agent do it. Install the `hotpath` CLI and run `init` inside your project repo:

```bash
cargo install hotpath --version '^{{HOTPATH_VERSION}}'
hotpath init --agent claude # or --agent codex
```

`hotpath init` downloads the [hotpath_init agent skill](https://github.com/pawurb/hotpath-rs/blob/main/skills/hotpath_init/SKILL.md) from GitHub and starts your installed Claude Code or Codex with it as setup instructions. The agent inspects your project, adds the dependency, instruments `main` and a starting set of functions, channels and locks, then verifies that everything compiles with profiling enabled and disabled.

Your agent remains in control: you review and approve edits through its regular permission prompts. Requires `curl` and the `claude` or `codex` CLI on `PATH`.

You can also install the skill directly, without the hotpath CLI:

```bash
mkdir -p ~/.claude/skills/hotpath_init
curl -fsSL https://raw.githubusercontent.com/pawurb/hotpath-rs/main/skills/hotpath_init/SKILL.md \
  -o ~/.claude/skills/hotpath_init/SKILL.md
```

Then run `/hotpath_init` in a Claude Code session.

### Manual installation

Add to your `Cargo.toml`:

```toml
[dependencies]
hotpath = "{{HOTPATH_VERSION}}"

[features]
hotpath = ["hotpath/hotpath"]
hotpath-cpu = ["hotpath/hotpath-cpu"]
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
    for i in 0..1000 {
        sync_function(i);
        async_function(i * 2).await;

        hotpath::measure_block!("custom_block", {
            std::thread::sleep(Duration::from_nanos(i * 3))
        });
    }
}
```

Now, run your program with `hotpath` (and optionally `hotpath-alloc`) feature:

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

threads - Thread CPU and memory statistics. (RSS: 7.8 MB, Alloc: 2.1 MB, Dealloc: 304.3 KB, Diff: 1.8 MB)
+--------------+----------+------+------+----------+---------+-----------+----------+----------+----------+
| Thread       | Status   | CPU% | Max% | CPU User | CPU Sys | CPU Total | Alloc    | Dealloc  | Diff     |
+--------------+----------+------+------+----------+---------+-----------+----------+----------+----------+
| main         | Sleeping | 6.3% | 6.3% | 0.123s   | 0.070s  | 0.193s    | 367.8 KB | 9.9 KB   | 357.9 KB |
+--------------+----------+------+------+----------+---------+-----------+----------+----------+----------+
| thread_5     | Sleeping | -    | -    | 0.000s   | 0.000s  | 0.000s    | 640 B    | 24 B     | 616 B    |
+--------------+----------+------+------+----------+---------+-----------+----------+----------+----------+
```

## Quick demo

Other than the SSH demo an easy way to quickly try the <a href="https://ratatui.rs/" target="_blank">ratatui.rs</a>-powered TUI is to run it in **auto-instrumentation mode**. The TUI process profiles itself and displays its own performance metrics in real time.

First, install `hotpath` CLI with auto-instrumentation enabled:

```bash
cargo install hotpath --features='tui,hotpath,hotpath-alloc' --version '^{{HOTPATH_VERSION}}'
```

Then launch the TUI:

```bash
hotpath
```

and you'll see timing, memory and other metrics.

Make sure to reinstall it without the auto-profiling features so that you can also observe metrics of other programs!

```bash
cargo install hotpath --features='tui' --version '^{{HOTPATH_VERSION}}'
```

## Learn more

Read the [complete guide to profiling Rust applications](https://hotpath.rs/blog/profiling-rust-guide) - a comprehensive overview of debugging performance issues in Rust.

Explore the docs for customization options and advanced profiling features.

- [Profiling modes](./profiling_modes.html) - static reports vs live TUI dashboard
- [Profiling overhead](./profiling_overhead.html) - per-operation instrumentation cost and time sampling
- [Functions](./functions.html) - measure execution time and memory allocations
- [CPU profiling](./cpu_profiling.html) - attribute CPU samples to instrumented functions
- [Threads](./threads.html) - monitor threads usage
- [Async Data Flow](./data_flow.html) - monitor channels, streams, and futures
- [Locks](./locks.html) - track Mutex and RwLock wait and hold times
- [SQL queries](./sql_tracing.html) - profile query execution time for sqlx and Diesel
- [HTTP requests](./http_tracing.html) - profile reqwest client calls per endpoint
- [I/O tracing](./io_tracing.html) - monitor bytes, throughput and duration of TCP, Redis, and file operations
- [Tokio Runtime](./tokio_runtime.html) - monitor Tokio runtime worker stats and task scheduling
- [Debug & Metrics](./debug.html) - track custom values with dbg!, val!, and gauge! macros
- [GitHub CI](./github_ci.html) - automated benchmarking and regression detection in CI
- [MCP Server](./mcp.html) - LLM integration via Model Context Protocol
- [Cargo flamegraph alternatives](/blog/sampling_comparison) - when to use `hotpath` vs sampling profilers like perf and samply
- [Configuration](./configuration.html) - explore all config options
