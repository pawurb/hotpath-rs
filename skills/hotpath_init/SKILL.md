---
name: hotpath_init
description: Configure hotpath profiling in a Rust project. Adds the hotpath dependency with feature-gated setup, instruments main with hotpath::main, functions with measure/measure_all, and wraps channels, mutexes, rw_locks, streams, futures, reqwest clients and byte-level I/O with hotpath macros. Use when the user wants to add or set up hotpath profiling in a crate.
allowed-tools: Bash, Read, Edit, Write, Glob, Grep
---

# Initialize hotpath Profiling

Set up [hotpath](https://hotpath.rs) profiling in the current Rust project. The setup is fully feature-gated: zero compile-time and runtime overhead unless the `hotpath` feature is explicitly enabled. All macros are noops when the feature is off, so no `cfg_attr` wrapping is needed.

## Steps

### 1. Inspect the project

- Find the binary crate(s) and the `main` function. If there is no `main` you control (e.g. a library or a test harness), use the `HotpathGuardBuilder` API instead of `#[hotpath::main]` (see step 3).
- Detect the async runtime (`tokio`, `smol`, none) and which instrumentable primitives the code uses: channels (`tokio::sync::mpsc`/`oneshot`, `std::sync::mpsc`, `crossbeam_channel`, `flume`, `async-channel`, `futures_channel`), `Mutex` and `RwLock` (std/parking_lot/tokio/async-lock), futures streams, sqlx, diesel, reqwest clients (async only; note which reqwest major - 0.12 or 0.13), byte-level I/O values implementing `std::io::Read`/`Write` or `tokio::io::AsyncRead`/`AsyncWrite` (files, sockets, compression codecs).

### 2. Add the dependency and feature passthrough

In the target crate's `Cargo.toml`:

```toml
[dependencies]
hotpath = "0.23"

[features]
hotpath = ["hotpath/hotpath"]
hotpath-alloc = ["hotpath/hotpath-alloc"]
```

Enable extra hotpath cargo features on the dependency based on what the project uses:

- `tokio` - for `tokio::sync` channel instrumentation, async `io!` traits (`AsyncRead`/`AsyncWrite`), and `hotpath::tokio_runtime!()` metrics: `hotpath = { version = "0.23", features = ["tokio"] }`
- `crossbeam` - for `crossbeam_channel` instrumentation
- `futures` - for `futures_channel` instrumentation
- `flume` - for `flume` channel instrumentation
- `async-channel` - for `async-channel` instrumentation
- `parking_lot` - for `parking_lot` `RwLock`/`Mutex` instrumentation
- `async-lock` - for `async-lock` `RwLock`/`Mutex` instrumentation
- `sqlx` - for SQL query profiling via `hotpath::sqlx_tracing_layer()`
- `diesel` - for SQL query profiling via `hotpath::instrument_diesel_sql()`
- `reqwest-0-12` / `reqwest-0-13` - for HTTP request profiling via `hotpath::http!(client)`; pick the feature matching the project's reqwest major version
- `axum-0-8` - for server-side response time profiling per axum 0.8 route via `hotpath::axum!(router)`

If the crate already has a `[features]` section, merge the entries.

### 3. Instrument main

`#[hotpath::main]` initializes the profiler and prints the report when main exits. With tokio, `#[tokio::main]` must come FIRST (above):

```rust
#[tokio::main]
#[hotpath::main]
async fn main() {
    // ...
}
```

Optional parameters: `#[hotpath::main(percentiles = [50, 95, 99.9], format = "json", limit = 20)]`. Defaults are fine for a first setup; don't add parameters unless asked.

If attribute placement on main is not possible, build a guard programmatically (report prints when the guard drops):

```rust
let _hotpath = hotpath::HotpathGuardBuilder::new("main")
    .build();
```

Unlike `#[hotpath::main]`, the builder does not install the allocation-tracking allocator, so with the guard builder also declare it as a static (required for `hotpath-alloc` to report anything):

```rust
#[global_allocator]
static GLOBAL: hotpath::CountingAllocator = hotpath::CountingAllocator::new();
```

No feature gating needed: `CountingAllocator` is a no-op pass-through when `hotpath-alloc` (or `hotpath`) is disabled. Never combine this static with `#[hotpath::main]` - the macro emits its own `#[global_allocator]` under `hotpath-alloc` and the build fails with a duplicate; with the macro, pass `allocator = ...` instead.

If the project already declares a custom global allocator (jemalloc, mimalloc, ...):

- With the guard builder: replace the existing static's type with the wrapper, e.g. `static GLOBAL: hotpath::CountingAllocator<tikv_jemallocator::Jemalloc> = hotpath::CountingAllocator::with(tikv_jemallocator::Jemalloc);`. The program keeps running on the custom allocator in all builds; tracking activates only under `hotpath-alloc`.
- With `#[hotpath::main]`: add the `allocator = tikv_jemallocator::Jemalloc` parameter and gate the project's own `#[global_allocator]` static behind `#[cfg(not(feature = "hotpath-alloc"))]` so the two never coexist.

### 4. Instrument functions

- Prefer `#[hotpath::measure_all]` on inline modules and `impl` blocks - it instruments every function inside. Exclude noisy or trivial functions with `#[hotpath::skip]`.
- Use `#[hotpath::measure]` on individual functions, both sync and async. 
- Useful parameters: `log = true` (log return values, requires `Debug`), `label = "name"` (custom identifier, duplicates panic at runtime).
- `hotpath::measure_block!("label", { ... })` for ad-hoc code blocks.

Start with hot paths: request handlers, worker loops, parsing/serialization, IO-heavy functions. Don't instrument one-line getters.

Async functions are measured runtime-agnostically, and under `hotpath-alloc` their allocations are tracked too (per-poll attribution via an async bridge), so no special handling is needed.

Don't try to instrument everything, use up to ~5 `hotpath::measure_all` annotations and up to 30 `hotpath::measure`. Goal of the initial setup is not to measure all functions, but to get the initial working instrumentation in place.

### 5. Wrap data-flow primitives

Wrap at the creation site; all wrappers accept optional `label = "name"` and (where noted) `log = true`:

```rust
// Channels (tokio mpsc/oneshot, std mpsc, crossbeam, flume, async-channel, futures_channel)
let (tx, rx) = hotpath::channel!(mpsc::channel::<String>(100), label = "jobs", log = true);
// tokio oneshot + futures_channel need proxy mode (compile error otherwise);
// bounded std sync_channel and futures_channel mpsc also need capacity = N (must match):
let (tx, rx) = hotpath::channel!(mpsc::channel::<String>(10), proxy = true, capacity = 10);

// Locks (wait time + held time)
let mutex = hotpath::mutex!(std::sync::Mutex::new(state), label = "state");
let lock = hotpath::rw_lock!(tokio::sync::RwLock::new(config), label = "config");

// Streams and futures
let s = hotpath::stream!(stream::iter(1..=10), label = "events");
let result = hotpath::future!(some_async_operation(), label = "fetch").await;

// Byte-level I/O (std Read/Write; AsyncRead/AsyncWrite require the `tokio` feature) -
// per-operation counts, bytes, transfer rate, durations, and errors
let mut file = hotpath::io!(std::fs::File::open("data.bin")?, label = "data-file");
let stream = hotpath::io!(tokio::net::TcpStream::connect(addr).await?, label = "conn");
```

Call-site aggregation (`channel!`, `stream!`, `io!`):

- By default all instances created at one call site (with the same message/item type) aggregate into a single report entry: counts, rates, and histograms are summed across instances, and an `Inst` column reports how many instances the entry aggregates. Profiler state stays bounded by the number of call sites, so this is safe for unbounded instance churn (a channel or stream per handled request, an `io!` wrapper per accepted connection).
- Aggregated channel/stream entries show `-` for state (instances open and close independently); single-instance entries keep their exact state.
- Disable aggregation with `iter = true` (e.g. `hotpath::channel!(mpsc::channel::<u32>(8), iter = true)`): every instance gets its own row (`label`, `label-2`, `label-3`, ...) with individual counts and rates - useful for one row per spawned worker. State then grows with the number of instances ever created, so avoid it for unbounded churn.

`io!` notes:

- Wrapping the underlying resource (file, socket) measures actual resource I/O; wrapping a `BufReader`/`BufWriter` measures application-facing buffered operations.
- The wrapper derefs to the wrapped value, so call sites don't change. For consuming methods (e.g. a codec's `finish(self)`), unwrap first with `hotpath::io_unwrap(x)` - identity when profiling is off, so call sites compile identically in both modes.
- Wrap the side where the work happens, or the reported rate is meaningless. Deferring writers (e.g. `brotli::CompressorWriter`) accept cheap buffered `write` calls and compress at finalization, outside any measured op; prefer read-side codec adapters (`flate2::read::GzEncoder`, `brotli::CompressorReader`, `zstd::stream::read::Encoder`), which compress inside instrumented `read` calls and report compressed bytes out.

Wrapped locks/channels are drop-in: the wrappers expose the same API, so call sites don't change. If passing them across function boundaries requires type-signature changes, note that to the user rather than rewriting half the codebase silently.

Wrapper macros return types prefixed with `hotpath::wrap`, make sure to update type signatures where needed. Explain to user that these types are no-op unless `hotpath` feature is enabled.

Apply `log = true` only if `Debug` is already implemented.

### 6. Optional extras (only when relevant)

- Tokio runtime metrics: call `hotpath::tokio_runtime!();` once at startup (requires `tokio` feature).
- SQL profiling (sqlx 0.8/0.9): add the layer to the tracing subscriber once - `tracing_subscriber::registry().with(hotpath::sqlx_tracing_layer()).init();` (requires `sqlx` feature). Don't filter out the `sqlx::query` target.
- SQL profiling (diesel): call `hotpath::instrument_diesel_sql();` once at startup (requires `diesel` feature).
- HTTP profiling (reqwest, async client only): wrap the client once at creation - `let client = hotpath::http!(reqwest::Client::new());` (requires `reqwest-0-12` or `reqwest-0-13` feature). Call sites stay unchanged; requests are reported per normalized endpoint (`GET host/path` with id-like segments collapsed to `{id}`) with an error count. Optional `label = "name"` prefixes endpoint keys - use it when the app has several clients. Where the client is stored in a struct or named in signatures, use `hotpath::wrap::reqwest::Client` (it resolves to the raw `reqwest::Client` when the feature is off). If the app already uses reqwest-middleware, attach `hotpath::ReqwestHttpMiddleware::new()` to its existing stack instead of the macro.
- axum server profiling: wrap the finished router once - `let app = hotpath::axum!(Router::new().route(..).route(..));` (requires `axum-0-8` feature). It expands to `router.layer(hotpath::AxumLayer::new())`, so it must come after the last `.route(..)`/`.fallback(..)` call; requests are reported per matched route template (`GET /users/{id}`) with 4xx/5xx counts. If the app already stacks tower layers on the router, add `.layer(hotpath::AxumLayer::new())` where it fits instead of the macro.

### 7. Verify

```bash
cargo check                       # feature off: must still compile, zero overhead
cargo check --features hotpath    # feature on
cargo run --features hotpath      # prints report on exit
```

Optionally verify alloc mode: `cargo run --features 'hotpath,hotpath-alloc'`.

Report what was instrumented and mention next steps: the live TUI (`cargo install hotpath --features tui`, then `hotpath console` while the app runs - metrics server listens on port 6770 by default), and `HOTPATH_OUTPUT_FORMAT=json` for machine-readable output. Report sections need no configuration: the default `HOTPATH_REPORT=auto` shows function and thread sections plus every instrumented section with data (channels, streams, futures, rw_locks, mutexes, sql, ...). Mention `HOTPATH_REPORT` only if the user wants to restrict output: an exact comma-separated list (e.g. `HOTPATH_REPORT=functions-timing,sql`), `all`, or auto with exclusions like `HOTPATH_REPORT=auto,-threads` / `HOTPATH_REPORT=-threads`.

Also explain to the user that hotpath is safe to keep as a regular (non-optional) dependency: unless the `hotpath` feature is enabled, it compiles zero third-party dependencies (only the hotpath crates themselves), and all macros expand to noops, so there is no compile-time bloat and no runtime overhead.

## Rules

- Never enable the `hotpath` feature by default (`default = []`); profiling must stay opt-in.
- Keep edits minimal: dependency, main, and a sensible starting set of instrumented functions/primitives. Expand coverage only when the user asks.
