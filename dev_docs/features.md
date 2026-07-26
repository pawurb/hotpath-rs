# Features Reference

Feature flags, macros, configuration API, and environment variables. See `CLAUDE.md` for project guidance.

## Feature Flags

The library supports different profiling modes via feature flags:
- `hotpath` - Basic time-based profiling
- `hotpath-alloc` - Track both bytes and allocation count per function simultaneously
- `hotpath-cpu` - Enable CPU sampling profiling via `samply` (macOS and Linux). Spawns the `hotpath-samply` wrapper binary on guard build; collects samples as gzipped Firefox Profiler JSON from the running process and attributes them to instrumented functions on shutdown or on-demand snapshot.
- `hotpath-mcp` - Enable MCP (Model Context Protocol) server for AI tool integration
- `tokio` - Enable tokio-specific integrations: `channel!` on tokio channels, tokio `Mutex`/`RwLock` wrappers, async `io!` traits, and `tokio_runtime!()` metrics. Not required for async function profiling itself, which works on any runtime.
- `futures` - Enable futures-channel support
- `crossbeam` - Enable crossbeam channel support
- `sqlx` - Enable SQL query profiling via `hotpath::sqlx_tracing_layer()`, a `tracing_subscriber::Layer` that observes sqlx's per-query `sqlx::query` events. No sqlx dependency is pulled in - the layer is generic over `tracing` and reads query telemetry from the event fields, a schema shared by sqlx 0.8 and 0.9, so one layer covers both. Captures transaction-internal queries; queries are keyed by *normalized* statement text.
- `toasty` - Enable SQL query profiling for the Toasty ORM via `hotpath::toasty_tracing_layer()`, a `tracing_subscriber::Layer` that observes toasty-core's per-query `toasty::query` events (OpenTelemetry-style fields: `db.statement`, `duration_ms`). No toasty dependency is pulled in. Covers all Toasty SQL drivers (SQLite, PostgreSQL, MySQL, Turso) including transaction-internal queries; key-value (DynamoDB) operations carry no SQL and are skipped.
- `diesel` - Enable SQL query profiling for Diesel via `hotpath::instrument_diesel_sql()`, a `diesel::connection::Instrumentation` callback registered before opening connections. Covers Postgres, MySQL, and SQLite. Feeds the same shared `sql` section as the sqlx/toasty layers.
- `reqwest-0-12` / `reqwest-0-13` - Enable HTTP request profiling for the matching reqwest version via `hotpath::http!(reqwest::Client::new())`. Wraps the client with reqwest-middleware's `ClientWithMiddleware` plus hotpath's timing middleware (`ReqwestHttpMiddleware` can also be attached to an existing middleware stack directly). Requests are bucketed by normalized endpoint (`METHOD host/path` with id-like segments collapsed to `{id}`); transport errors and 4xx/5xx responses count in `Errors`.
- `parking_lot` / `async-lock` - Enable `rw_lock!`/`mutex!` wrapper support for the parking_lot and async-lock crates
- `flume` / `async-channel` - Enable `channel!` support for flume and async-channel channels
- `threads` - Enable thread monitoring (default feature)
- `tui` - Enable live TUI console for real-time monitoring (requires building the binary)
- `demo` - Adds live sqlx, diesel (SQLite) and reqwest traffic to the TUI demo (`console` command). Implies `hotpath` and pulls the sqlx/diesel clients only here - never in plain `tui` or library `hotpath` builds. Run the demo with `cargo run --bin hotpath --features tui,demo -- console`.
- `utils` - Enable `hotpath-utils` CLI binary for CI integration

## Running with Different Profiling Modes

```bash
# Time-based profiling (Tokio)
cargo run -p test-tokio-async --features=hotpath --example basic

# Memory allocation profiling (Tokio)
cargo run -p test-tokio-async --features='hotpath,hotpath-alloc' --example basic

# Async function profiling with smol
cargo run -p test-smol-async --features=hotpath --example basic_smol

# Async function profiling with Tokio
cargo run -p test-tokio-async --features='hotpath,tokio' --example async_multithread

# Channel monitoring examples
cargo run -p test-channels-tokio --example basic_tokio --features hotpath
cargo run -p test-channels-crossbeam --example basic_crossbeam --features hotpath
```

## Macro System

- `#[hotpath::main]` - Initializes background profiling system and generates final report. Parameters: `percentiles = [50, 95, 99.9]`, `format = "json"`, `limit = 20` (plus per-resource variants like `functions_limit`, `channels_limit`, ...), `output_path = "report.json"`, `report = "..."`, `allocator = ...`. For a timed shutdown use `HOTPATH_SHUTDOWN_MS` or `build_with_shutdown`.
- `#[hotpath::measure]` - Instruments functions with appropriate guard (time or allocation). Parameters: `log = true` (logs return value), `future = true` (also emits future lifecycle events for async fns), `label = "name"` (replaces full reported identifier; duplicates panic at runtime), `impl_type = "Type"` (inserts the enclosing type segment so the registered name is `module::Type::fn_name`; required for correct `hotpath-cpu` attribution when applying bare `#[measure]` to a method inside an `impl` not covered by `measure_all`)
- `#[hotpath::measure_all]` - Instruments all functions in a module or impl block. On inherent impl blocks the type segment is auto-injected (`module::Type::method`) so CPU sampling attribution matches the demangled symbol. Trait impls (`impl Trait for Type`) are instrumented but their demangled symbols use `<Type as Trait>::method`, so CPU attribution won't match for trait methods.
- `#[hotpath::skip]` - Excludes a function from profiling when using measure_all
- `#[hotpath::future_fn]` - Instruments async functions for future lifecycle tracking. Parameter: `log = true`
- `hotpath::measure_block!("label", { ... })` - Instruments code blocks
- `hotpath::rw_lock!(expr)` - Wraps a `RwLock` (std/parking_lot/tokio/async-lock) to track read/write wait & acquire time. Parameter: `label = "name"`
- `hotpath::mutex!(expr)` - Wraps a `Mutex` (std/parking_lot/tokio/async-lock) to track lock wait & acquire time (single lock kind, no read/write split). Parameter: `label = "name"`
- `hotpath::future!(expr)` - Wraps a future for lifecycle tracking
- `hotpath::sqlx_tracing_layer()` - A `tracing_subscriber::Layer` that captures sqlx query telemetry (requires `sqlx` feature). Add once to your `tracing` subscriber: `tracing_subscriber::registry().with(hotpath::sqlx_tracing_layer()).init();`. Works with sqlx 0.8 and 0.9. Don't globally filter out the `sqlx::query` target.
- `hotpath::toasty_tracing_layer()` - Same pattern for the Toasty ORM (requires `toasty` feature): a `tracing_subscriber::Layer` that captures toasty-core's `toasty::query` events. Don't globally filter out the `toasty::query` target.
- `hotpath::instrument_diesel_sql()` - Registers a `diesel::connection::Instrumentation` callback for Diesel SQL query profiling (requires `diesel` feature). Call once before opening connections.
- `hotpath::http!(expr)` - Wraps a `reqwest::Client` for per-endpoint HTTP request profiling (requires `reqwest-0-12` or `reqwest-0-13` feature). Parameter: `label = "name"` (prefixes every endpoint key). The `hotpath::wrap::reqwest::Client` type alias (`hotpath::wrap::reqwest_012::Client` for 0.12) resolves to `ClientWithMiddleware` when profiling is on and the raw client when off, so struct fields keep one written type. Times reqwest's `execute` future - resolves when response headers arrive, so body download is excluded.
- `hotpath::io!(expr)` - Wraps any `std::io::Read`/`Write` or `tokio::io::AsyncRead`/`AsyncWrite` value (async traits require the `tokio` feature) to track per-operation-kind (read, write, flush, shutdown) counts, bytes, per-operation transfer rate, durations, and errors. Parameters: `label = "name"`, `iter = true` (per-instance rows `label`, `label-2`, ... instead of the default aggregation by creation site; unbounded instance churn then grows profiler state). Derefs to the wrapped value; `into_inner()` is the escape hatch for consuming methods. `Seek`/`BufRead` delegation is not instrumented.
- `hotpath::tokio_runtime!()` - Initializes Tokio runtime metrics monitoring (requires `tokio` feature)
- `hotpath::dbg!(expr)` - Like `std::dbg!` but sends output to profiler debug tab instead of stderr
- `hotpath::val!("key").set(&value)` - Tracks key-value pairs in debug tab (requires `Debug` trait)
- `hotpath::gauge!("name").set(42.0)` - Tracks numeric values with set/inc/dec operations in debug tab
- All guards have `#[must_use]` attribute to prevent accidental drops

## HotpathGuard Builder API

Programmatic configuration via `HotpathGuardBuilder`:

```rust
HotpathGuardBuilder::new("main")
    .percentiles(&[50.0, 95.0, 99.9])
    .functions_limit(10)     // max functions in report (default: 15)
    .channels_limit(5)       // max channels in report (default: 0)
    .streams_limit(5)        // max streams in report (default: 0)
    .futures_limit(5)        // max futures in report (default: 0)
    .rw_locks_limit(5)       // max rw_locks in report (default: 0)
    .mutexes_limit(5)        // max mutexes in report (default: 0)
    .sql_limit(5)            // max SQL queries in report (default: 0)
    .http_limit(5)           // max HTTP endpoints in report (default: 0)
    .io_limit(5)             // max io entries in report (default: 0)
    .threads_limit(5)        // max threads in report (default: 5)
    .format(Format::Json)
    .output_path("report.json")
    .sections(vec![Section::FunctionsTiming, Section::Channels])
    .before_shutdown(|| { /* cleanup before report generation */ })
    .build()
```

- `build()` returns a `HotpathGuard` (dropped on scope exit to generate report)
- `build_with_shutdown(Duration)` auto-shuts down after the given duration (configurable via `HOTPATH_SHUTDOWN_MS` env var)

## Channel and Stream Monitoring

The `channel!` and `stream!` macros provide instrumentation for async channels and streams, with real-time monitoring via the TUI.

*Channel Macro*: Wraps channel creation to track statistics

```rust
use tokio::sync::mpsc;

// Basic usage - automatically tracks sent/received counts
let (tx, rx) = hotpath::channel!(mpsc::channel::<String>(100));

// With custom label for easier identification
let (tx, rx) = hotpath::channel!(mpsc::channel::<String>(100), label = "my_channel");

// With message logging (requires Debug trait on messages)
let (tx, rx) = hotpath::channel!(mpsc::channel::<String>(100), log = true);

// Combined options
let (tx, rx) = hotpath::channel!(mpsc::channel::<String>(100), label = "my_channel", log = true);
```

Supported channel types: `tokio::sync::mpsc` (bounded/unbounded), `tokio::sync::oneshot`, `futures_channel::mpsc` (bounded/unbounded), `futures_channel::oneshot`, `crossbeam_channel` (bounded/unbounded), `flume` (bounded/unbounded), `async_channel` (bounded/unbounded), `std::sync::mpsc`

`futures_channel::mpsc` has no wrap implementation and is supported only through the forwarder path, and bounded channels need the `capacity` parameter: `hotpath::channel!(mpsc::channel::<String>(10), proxy = true, capacity = 10)`

Bounded `std::sync::mpsc::sync_channel` also requires `capacity = N` in the default wrap mode (std exposes no capacity accessor, so the wrapper rebuilds the channel with it; it must match the `sync_channel(N)` argument) - omitting it panics at runtime: `hotpath::channel!(mpsc::sync_channel::<String>(100), capacity = 100)`

Channel metrics tracked:
- Messages sent/received counts
- Current queue size, queued bytes, and max queue size
- Channel state (active, closed, notified); fullness is derived from queue depth vs capacity, not a state
- Message type and size
- Optional message logs (recent send/receive history)

*Stream Macro*: Wraps futures streams to track items yielded

```rust
use futures::stream::{self, StreamExt};

// Basic usage - tracks items yielded
let s = hotpath::stream!(stream::iter(1..=10));

// With custom label
let s = hotpath::stream!(stream::iter(1..=10), label = "my_stream");

// With item logging (requires Debug trait on items)
let s = hotpath::stream!(stream::iter(1..=10), log = true);
```

Stream metrics tracked:
- Items yielded count
- Stream state (active, closed)
- Item type and size
- Optional item logs (recent yield history)

*Future Macro*: Wraps futures to track lifecycle and poll counts

```rust
use hotpath::future;

// Basic usage - tracks poll counts and completion
let result = future!(some_async_operation()).await;

// With custom label
let result = future!(some_async_operation(), label = "my_future").await;

// With output logging (requires Debug trait on output)
let result = future!(some_async_operation(), log = true).await;
```

Future metrics tracked:
- Poll counts
- Future state (active, completed, cancelled)
- Optional output logs

## Async Support

- Async function profiling works on any async runtime (tokio, smol, ...) with no runtime-specific feature flag - instrumentation happens at poll boundaries, which is runtime-agnostic. See `crates/test-smol-async`, which uses only the `hotpath` feature.
- The `tokio` feature is only needed for tokio-specific integrations: `channel!` on tokio channels, tokio `Mutex`/`RwLock` wrappers, async `io!` traits (`tokio::io::AsyncRead`/`AsyncWrite`), and `tokio_runtime!()` metrics
- Time profiling: Full async support
- Allocation profiling: Full async support on any async runtime. Allocations are measured at poll boundaries - `measure_poll_alloc` in `lib_on/futures/wrapper.rs` wraps each `poll()` with `push_alloc_stack()`/`pop_alloc_stack()`, and per-poll totals are aggregated across threads via `AsyncAllocBridge` (`lib_on/functions/alloc/guard.rs`), which the async measurement guard snapshots on drop. Allocations during future construction, before the first poll, are not guaranteed to be counted. Nested measurements follow the same exclusive-by-default semantics (`HOTPATH_ALLOC_CUMULATIVE` rolls child polls into the parent).

## Environment Variables

Output:
- `HOTPATH_OUTPUT_FORMAT` - Output format: `table` (default), `json`, `json-pretty`, or `none` (silences output while keeping metrics server and MCP server active)
- `HOTPATH_OUTPUT_PATH` - File path for profiling reports. Takes precedence over programmatic `output_path` config. (default: `stdout`)
- `HOTPATH_REPORT` - Report sections spec: `all`, `auto`, an exact comma-separated list (`functions-timing`, `functions-alloc`, `functions-cpu`, `channels`, `streams`, `futures`, `rw_locks`, `mutexes`, `sql`, `http`, `io`, `threads`, `debug`), or auto with exclusions like `auto,-threads` / `-threads`. (default: `auto` - function and thread sections plus every instrumented section with data at shutdown)
- `HOTPATH_REPORT_LABEL` - Annotate reports with a label (e.g. git branch and commit hash)

Limits:
- `HOTPATH_LIMIT` - Maximum items shown in every report section. Set to `0` for unlimited. Per-resource env vars take precedence.
- `HOTPATH_FUNCTIONS_LIMIT` - Maximum functions in report (default: 15)
- `HOTPATH_CHANNELS_LIMIT` - Maximum channels in report (default: 0, unlimited)
- `HOTPATH_STREAMS_LIMIT` - Maximum streams in report (default: 0, unlimited)
- `HOTPATH_FUTURES_LIMIT` - Maximum futures in report (default: 0, unlimited)
- `HOTPATH_RW_LOCKS_LIMIT` - Maximum rw_locks in report (default: 0, unlimited)
- `HOTPATH_MUTEXES_LIMIT` - Maximum mutexes in report (default: 0, unlimited)
- `HOTPATH_SQL_LIMIT` - Maximum SQL queries in report (default: 0, unlimited)
- `HOTPATH_HTTP_LIMIT` - Maximum HTTP endpoints in report (default: 0, unlimited)
- `HOTPATH_IO_LIMIT` - Maximum io entries in report (default: 0, unlimited)
- `HOTPATH_THREADS_LIMIT` - Maximum threads in report (default: 5)
- The CPU sampling report has no dedicated limit variable - it uses the functions limit (`HOTPATH_FUNCTIONS_LIMIT`, falling back to `HOTPATH_LIMIT`). Wrapper `caller_name` is always shown and exempt from the limit.

Functions:
- `HOTPATH_FOCUS` - Filter profiled functions by name. Plain text does substring matching; wrap in `/pattern/` for regex (e.g. `HOTPATH_FOCUS="/^(compute|process)/"`).
- `HOTPATH_EXCLUDE_WRAPPER` - Set to "true" or "1" to calculate ratios using sum of measured functions instead of wrapper total (percentages will sum to ~100%)
- `HOTPATH_ALLOC_CUMULATIVE` - Set to "true" or "1" to track cumulative memory allocations per function (including nested calls) instead of the default exclusive mode. Produces invalid results for recursive functions.
- `HOTPATH_ALLOC_METRIC` - Primary allocation metric: `bytes` (default) or `count`. Controls which metric drives sorting and percentages in allocation reports; any other value panics.
- `HOTPATH_CPU_INCLUSIVE` - Set to "true" or "1" to attribute each CPU sample to every instrumented function in its stack (parent functions get credit for time spent in callees) instead of the default exclusive mode. Recursive frames are deduped per sample.
- `HOTPATH_CPU_BASELINE_OFF` - Set to "true" or "1" to disable CPU baseline collection
- `HOTPATH_FUNCTIONS_NAME_DEPTH` - Number of trailing module segments to keep when displaying function names (`1` = function name only, `2` = current default with one module, `0` = unlimited full path)

Time sampling:
- `HOTPATH_TIME_SAMPLING_RATE` - Fraction (0.0-1.0) of operations to time; skipped operations avoid the clock reads while call counts and queue sizes stay exact. `0.0` gives count-only mode (durations and io `Rate` show `-`).
- Per-resource variants take precedence: `HOTPATH_FUNCTIONS_TIME_SAMPLING_RATE`, `HOTPATH_CHANNELS_TIME_SAMPLING_RATE`, `HOTPATH_FUTURES_TIME_SAMPLING_RATE`, `HOTPATH_RW_LOCKS_TIME_SAMPLING_RATE`, `HOTPATH_MUTEXES_TIME_SAMPLING_RATE`, `HOTPATH_IO_TIME_SAMPLING_RATE`

Metrics Server:
- `HOTPATH_METRICS_PORT` - Port for metrics HTTP server (default: 6770)
- `HOTPATH_METRICS_SERVER_OFF` - Set to "true" or "1" to disable the HTTP metrics server entirely

MCP Server:
- `HOTPATH_MCP_PORT` - Port for MCP server (default: 6771)
- `HOTPATH_MCP_AUTH_TOKEN` - Optional authentication token for MCP server

TUI:
- `HOTPATH_TUI_REFRESH_INTERVAL_MS` - TUI dashboard refresh interval in milliseconds (default: 500)
- `HOTPATH_TUI_TAB` - Initial top-level tab to display when launching the TUI, as a number `1`-`6` (`1` Functions, `2` Data Flow, `3` I/O, `4` Threads, `5` Debug, `6` Runtime); invalid values fall back to Functions
- `HOTPATH_METRICS_HOST` - Host URL that the TUI console connects to (default: `http://localhost`)
- `HOTPATH_DISABLE_SAMPLY_LOAD` - Set to "true" or "1" to disable the `samply load` shortcut on the CPU subtab; the `'f'` keybinding and its hint are hidden
- `HOTPATH_TUI_AUTO_EXPAND_LOGS` - Auto-open the logs panel once initial data arrives and pin selection to the given table index (e.g. `0` for the first row; default: unset)

CPU profiling (`hotpath-cpu` feature, macOS and Linux):
- `HOTPATH_SAMPLY_WRAPPER_BIN` - Path to the `hotpath-samply` wrapper binary that autospawn launches (default: `hotpath-samply`, resolved via `PATH`)
- `HOTPATH_SAMPLY_BIN` - Path to the external `samply` binary used by the `hotpath-samply` worker (default: `samply`, resolved via `PATH`)
- `HOTPATH_CPU_INCLUSIVE` - Set to "true" or "1" to attribute CPU samples inclusively (parent functions credited for callee time)
- `HOTPATH_CPU_BASELINE_OFF` - Set to "true" or "1" to disable CPU baseline collection
- `HOTPATH_KEEP_INLINE` - Set to "true" or "1" to disable the macro's inline-attribute rewrite (`#[hotpath::measure]`/`#[hotpath::future_fn]` strip user-provided `#[inline(...)]` and inject `#[inline(never)]` under `hotpath-cpu` so symbols match for CPU attribution). Read at proc-macro expansion time - touch source or `cargo clean` after toggling.

Dev logging (`dev` feature):
- `HOTPATH_DEV_LOG_PATH` - Path to the development log file (default: `log/development.log`). Honored by `tracing` subscriber initialized in `HotpathGuardBuilder::build()` and `hotpath-samply` worker
- Standard `RUST_LOG` env filter applies (default level: `error`)

Other:
- `HOTPATH_DRAIN_INTERVAL` - Interval in milliseconds between background worker sweeps of the per-thread event queues (default: 50). Decrease for high-traffic apps to bound queue memory growth between sweeps.
- `HOTPATH_THREADS_INTERVAL_MS` - Thread monitoring sample interval in milliseconds (default: 250)
- `HOTPATH_TOKIO_RUNTIME_INTERVAL_MS` - Tokio runtime metrics sampling interval in milliseconds (default: 1000)
- `HOTPATH_LOGS_LIMIT` - Maximum number of log entries to keep per channel, stream, or function (default: 50)
- `HOTPATH_MAX_LOG_LEN` - Maximum character length for logged return values (`log = true`). Values exceeding this limit are truncated with `...` (default: 1536)
- `HOTPATH_SHUTDOWN_MS` - If set, program will shutdown after the specified ms timeout and print the performance report
- `HOTPATH_SQL_RAW_LOGS` - Set to "true" or "1" to store raw statement text in per-query SQL logs instead of the normalized form. Off by default so bound literals (potentially sensitive) never reach the logs.

## GitHub CI Integration

The `hotpath-utils profile-pr` command compares PR branch metrics against base and posts a diff comment on the PR. Uses a two-workflow setup (`hotpath-profile` + `hotpath-comment`) for fork security.
