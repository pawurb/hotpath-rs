# Architecture Reference

Runtime internals: background workers, servers, and CPU sampling. See `CLAUDE.md` for project guidance and the high-level pipeline description.

## Background Processing

The profiler uses dedicated background worker threads for each subsystem:
- `hp-functions` - Function measurements swept from per-thread SPSC queues (shutdown/query control channels wake it early via `Select::ready_timeout`), HDR histograms for percentiles
- `hp-channels` - Channel events swept from per-thread SPSC queues
- `hp-streams` - Stream events swept from per-thread SPSC queues
- `hp-futures` - Future events swept from per-thread SPSC queues
- `hp-rw-locks` - RwLock events swept from per-thread SPSC queues
- `hp-mutexes` - Mutex events swept from per-thread SPSC queues
- `hp-sql` - SQL query events swept from per-thread SPSC queues
- `hp-http` - HTTP request events swept from per-thread SPSC queues
- `hp-io` - Byte-level I/O events swept from per-thread SPSC queues
- `hp-debug` - Debug entries (`dbg!`, `val!`, `gauge!`)
- `hp-threads` - Thread monitoring with configurable sampling interval
- `hp-runtime` - Tokio runtime metrics sampling (configurable interval, default 1s)

Each worker:
1. Receives measurements from the main application
2. Maintains running statistics
3. Generates performance report on program shutdown
4. Handles graceful shutdown with measurement draining

## HTTP Metrics Server

A non-authenticated HTTP server starts automatically by default on port 6770, binding to localhost (127.0.0.1) only. This server exposes real-time metrics for TUI integration and external monitoring tools. Implementation: `crates/hotpath/src/metrics_server.rs`; the route table is the `Route` enum in `crates/hotpath/src/json.rs`.

Configuration:
- `HOTPATH_METRICS_PORT` - Customize the port (default: 6770)
- `HOTPATH_METRICS_SERVER_OFF=true` - Disable the server entirely

Server details:
- Runs on a separate thread using tiny_http
- Binds to 127.0.0.1 only (not accessible from external hosts)
- No authentication required (localhost access only)

Endpoints:
- `GET /functions_timing` - Returns function timing metrics as JSON
- `GET /functions_alloc` - Returns function allocation metrics as JSON (requires `hotpath-alloc` feature)
- `GET /functions_cpu` - Returns CPU sampling attribution as JSON (requires `hotpath-cpu` feature)
- `GET /functions_timing/{id}/logs` - Returns recent timing logs for a specific function
- `GET /functions_alloc/{id}/logs` - Returns recent allocation logs for a specific function
- `GET /channels` - Returns channel metrics as JSON
- `GET /channels/{id}/logs` - Returns channel logs for a specific channel
- `GET /streams` - Returns stream metrics as JSON
- `GET /streams/{id}/logs` - Returns stream logs for a specific stream
- `GET /futures` - Returns future metrics as JSON
- `GET /futures/{id}/logs` - Returns future logs for a specific future
- `GET /rw_locks` - Returns RwLock read/write wait & acquire-time metrics as JSON
- `GET /mutexes` - Returns Mutex wait & acquire-time metrics as JSON
- `GET /sql` - Returns SQL query execution-time metrics (per normalized query) as JSON
- `GET /sql/{id}/logs` - Returns execution logs for a specific SQL query
- `GET /http` - Returns HTTP request metrics (per normalized endpoint) as JSON
- `GET /http/{id}/logs` - Returns request logs for a specific HTTP endpoint
- `GET /io` - Returns byte-level I/O metrics (per `io!` entry, per operation kind) as JSON
- `GET /threads` - Returns thread CPU usage metrics (requires `threads` feature)
- `GET /tokio_runtime` - Returns Tokio runtime metrics snapshot (requires `tokio` feature)
- `GET /debug` - Returns debug entries

Used by TUI console for real-time monitoring.

## MCP Server

The library includes an MCP (Model Context Protocol) server for AI tool integration (`crates/hotpath/src/mcp_server.rs`). Requires `hotpath-mcp` feature:
- Runs on port 6771 by default (configurable via `HOTPATH_MCP_PORT`)
- Optional authentication via `HOTPATH_MCP_AUTH_TOKEN`: the env var sets the expected value on the server; clients must send that exact value as the standard `Authorization` header (compared verbatim, no `Bearer` prefix handling)
- Endpoint: `POST /mcp` (Streamable HTTP transport)

MCP Tools:
- `functions_timing` - Get execution timing metrics (call count, total/mean/p50/p95/p99 latencies)
- `functions_alloc` - Get memory allocation metrics per function (requires `hotpath-alloc` feature)
- `functions_cpu` - Get CPU sampling attribution per function (requires `hotpath-cpu` feature)
- `functions_cpu_snapshot` - Trigger an on-demand CPU sampling snapshot (requires `hotpath-cpu` feature)
- `channels` - Get channel metrics (sent/received counts, queue size, state)
- `streams` - Get stream metrics (items yielded, state)
- `futures` - Get future lifecycle metrics (poll counts, state)
- `rw_locks` - Get RwLock read/write wait & acquire-time metrics
- `mutexes` - Get Mutex wait & acquire-time metrics
- `sql` - Get SQL query execution-time metrics per normalized query
- `sql_logs` - Get detailed execution logs for a specific SQL query
- `http` - Get HTTP request metrics per normalized endpoint
- `http_logs` - Get detailed request logs for a specific HTTP endpoint
- `io` - Get byte-level I/O metrics per `io!` entry
- `threads` - Get thread CPU usage metrics
- `gauges` - Get custom gauge values
- `function_timing_logs` - Get detailed timing logs for a specific function
- `function_alloc_logs` - Get detailed allocation logs for a specific function
- `channel_logs` - Get message logs for a specific channel
- `stream_logs` - Get item logs for a specific stream
- `future_logs` - Get poll/completion logs for a specific future
- `gauge_logs` - Get gauge value history
- `dbg_entries` - Get all dbg! debug entries
- `val_entries` - Get all val! value tracking entries
- `dbg_logs` - Get detailed logs for a specific dbg! entry
- `val_logs` - Get detailed logs for a specific val! entry
- `tokio_runtime` - Get Tokio runtime metrics snapshot (requires `tokio` feature)
- `profiler_status` - Get profiler uptime status

## Tokio Runtime Monitoring

When the `tokio` feature is enabled, `hotpath::tokio_runtime!()` initializes runtime metrics collection:
- Spawns a dedicated `hp-runtime` background thread that polls `tokio::runtime::RuntimeMetrics`
- Stores snapshots in a static `RwLock` for retrieval via `get_runtime_json()`
- Metrics include per-worker stats (park count, busy duration) and global stats (alive tasks, queue depths)
- Additional metrics (poll counts, steal operations, blocking threads, IO driver stats) require building with `RUSTFLAGS="--cfg tokio_unstable"`
- Exposed via `GET /tokio_runtime` HTTP endpoint and rendered in the TUI runtime panel

## CPU Sampling (`hotpath-cpu` feature, macOS and Linux)

When `hotpath-cpu` is enabled, the profiler attributes CPU samples to instrumented functions using an external `samply` worker:

- **Wrapper binary**: `hotpath-samply` (built as `crates/hotpath/bin/hotpath-samply/main.rs`). Resolves `samply` via `PATH` (override with `HOTPATH_SAMPLY_BIN`). The host process spawns the wrapper as a child, which then `samply record --pid <host> --save-only -o hp.json.gz`s the host and writes a gzipped Firefox Profiler "processed profile" JSON under `/tmp/hotpath/<session_id>/hp.json.gz`. Note: this is samply's native JSON format (not Google pprof) - the on-disk file is gzipped JSON with `meta`, `libs`, and per-thread columnar tables.
- **Autospawn**: `crates/hotpath/src/lib_on/functions/cpu/autospawn.rs` manages the wrapper child. `start()` is called from `HotpathGuard::new()` when the `hotpath-cpu` feature is enabled, and re-invoked after each on-demand snapshot. `stop()` writes the `stop-profiling` sentinel, waits on the `<session>/done` marker, and returns the profile path. Override the wrapper binary path via `HOTPATH_SAMPLY_WRAPPER_BIN`.
- **Report build**: `crates/hotpath/src/lib_on/functions/cpu/samply.rs` decompresses `hp.json.gz` (via `flate2`), deserializes the Firefox Profiler JSON into the `Profile` struct (`serde_json`), then walks each thread's `samples` -> `stackTable` (linked-list of frames via `prefix`/`frame`) -> `frameTable` (relative virtual address + func) -> `funcTable.resource` -> `resourceTable.lib` -> `libs[]`. To resolve addresses to symbols, it parses the on-disk binary via `object` (using `lib.debug_path`/`path`), demangles symbols with `rustc-demangle`, strips `::h<hash>` suffixes, and builds a `LibSymbolIndex` of `(start, end, instrumented_display_name)` ranges; lookups use `partition_point` for O(log n) range search. Only the primary library (matching `current_exe` basename) is indexed. Symbols match longest-prefix, so closure suffixes (`::{{closure}}`) and hash suffixes attribute back to the parent function. Trait impl symbols (`<Type as Trait>::method`) are NOT matched - only inherent impl/free function names are. Per-sample weight comes from `threadCPUDelta` (µs) when present, falling back to `weight` or 1.
- **Attribution modes**: default is exclusive (each sample credited to the deepest matching function). With `HOTPATH_CPU_INCLUSIVE=1`, every matching frame in the stack is credited (parent functions get callee time); recursive frames are deduped per sample.
- **Final report**: on `Drop`, the guard calls `autospawn::stop()` and `build_cpu_report_from_path()` to produce a `CpuReport`, then renders it via `report_functions_cpu_table` (table format) or serializes through `JsonFunctionsCpuList` into `JsonReport.functions_cpu`.
- **On-demand snapshots**: `POST /functions_cpu/snapshot` triggers a background snapshot thread (`hp-cpu-snapshot`). Status is exposed via `GET /functions_cpu` as a `JsonFunctionsCpuEnvelope` (`status: idle|capturing|ready|error`, plus session/profile metadata). The TUI's CPU subtab uses these endpoints - `c` triggers a capture, `f` opens the last profile in `samply load`.
- **Macro requirement for impl methods**: applying bare `#[hotpath::measure]` to a method inside an `impl` block (rather than using `#[measure_all]`) requires the `impl_type = "TypeName"` parameter so the registered name is `module::Type::method` and matches the demangled symbol. `#[measure_all]` on inherent impls auto-injects this.

## Global State Management

- Single `OnceLock<Arc<RwLock<FunctionsState>>>` ensures `init()` called only once
- Thread-safe state sharing between main thread and background worker
- Statistics ownership transferred to background thread for performance
