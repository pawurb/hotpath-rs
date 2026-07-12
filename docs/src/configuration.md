# Environment Variables Configuration

`hotpath` behavior can be customized via environment variables. These take precedence over programmatic configuration (`hotpath::main` macro parameters and builder API).

## Output

| Variable | Description |
|----------|-------------|
| `HOTPATH_OUTPUT_FORMAT` | Output format: `table`, `json`, `json-pretty`, or `none`. Using `none` silences output while keeping the metrics server and MCP server active. (default: `table`) |
| `HOTPATH_OUTPUT_PATH` | Filesystem path for profiling reports. If unset, reports are written to `stdout`. When set, this env var takes precedence over programmatic `output_path` config. On Unix, use `/dev/stdout` or `/dev/stderr` to redirect to the standard streams. |
| `HOTPATH_REPORT` | Comma-separated sections to include in report: `functions-timing`, `functions-alloc`, `channels`, `streams`, `futures`, `threads`, `tokio_runtime`, `debug`, or `all`. (default: `functions-timing,functions-alloc,threads`) |

## Limits

| Variable | Description |
|----------|-------------|
| `HOTPATH_LIMIT` | Maximum number of items shown in every report section (functions, channels, streams, futures, threads). Set to `0` for unlimited. Per-resource env vars (e.g. `HOTPATH_FUNCTIONS_LIMIT`) take precedence. (default: unset) |
| `HOTPATH_FUNCTIONS_LIMIT` | Maximum number of functions shown in the report. Set to `0` for unlimited. (default: `15`) |
| `HOTPATH_CHANNELS_LIMIT` | Maximum number of channels shown in the report. Set to `0` for unlimited. (default: `0`) |
| `HOTPATH_STREAMS_LIMIT` | Maximum number of streams shown in the report. Set to `0` for unlimited. (default: `0`) |
| `HOTPATH_FUTURES_LIMIT` | Maximum number of futures shown in the report. Set to `0` for unlimited. (default: `0`) |
| `HOTPATH_THREADS_LIMIT` | Maximum number of threads shown in the report. Set to `0` for unlimited. (default: `5`) |

## Functions

| Variable | Description |
|----------|-------------|
| `HOTPATH_FOCUS` | Filter profiled functions by name. Plain text does substring matching; wrap in `/pattern/` for regex (e.g. `HOTPATH_FOCUS="/^(compute\|process)/"`). (default: `''`) |
| `HOTPATH_EXCLUDE_WRAPPER` | Set to `true` or `1` to calculate ratios using the sum of measured functions instead of the wrapper total. (default: `false`) |
| `HOTPATH_ALLOC_CUMULATIVE` | Set to `true` or `1` to track cumulative memory allocations per function (including nested calls) instead of the default exclusive mode. Produces invalid results for recursive functions. (default: `false`) |
| `HOTPATH_ALLOC_METRIC` | Primary metric for alloc mode: `bytes` or `count`. Controls sorting, percentages, and displayed values in reports. (default: `bytes`) |
| `HOTPATH_CPU_BASELINE_OFF` | Set to `true` or `1` to disable CPU baseline collection. (default: `false`) |
| `HOTPATH_KEEP_INLINE` | Set to `true` or `1` to preserve user-provided `#[inline(...)]` attributes instead of rewriting measured functions to `#[inline(never)]` under `hotpath-cpu`. This env var is read during proc-macro expansion, so run `cargo clean` before rebuilding for changes to take effect. (default: `false`) |
| `HOTPATH_FUNCTIONS_NAME_DEPTH` | Number of module segments to keep when displaying function names (including the function name itself). `1` = function name only, `2` = one module + function, `0` = unlimited (full path). When using the TUI, set this env var for the TUI process too, since the console applies name shortening in its own process. (default: `2`) |

## Time Sampling

Measure durations for only a fraction of calls to reduce profiling overhead in extremely hot code paths. Rates are fractions in `[0.0, 1.0]`: `0.1` times 1 in 10 calls, `0.0` is count-only mode (counts, states, and queue sizes stay exact, no durations at all), `1.0` or unset measures everything. Per-resource variables take precedence over the global rate, and all env vars take precedence over the [`HotpathGuardBuilder`](https://docs.rs/hotpath/latest/hotpath/struct.HotpathGuardBuilder.html) setters. See [Profiling overhead](profiling_overhead.md#reducing-overhead-time-sampling) for details.

| Variable | Description |
|----------|-------------|
| `HOTPATH_TIME_SAMPLING_RATE` | Global sampling rate applied to all resource types below. (default: unset, measure everything) |
| `HOTPATH_FUNCTIONS_TIME_SAMPLING_RATE` | Sampling rate for function timings. (default: unset) |
| `HOTPATH_MUTEXES_TIME_SAMPLING_RATE` | Sampling rate for mutex wait & acquire timings. (default: unset) |
| `HOTPATH_RW_LOCKS_TIME_SAMPLING_RATE` | Sampling rate for RwLock wait & acquire timings. (default: unset) |
| `HOTPATH_FUTURES_TIME_SAMPLING_RATE` | Sampling rate for future poll timings. (default: unset) |
| `HOTPATH_CHANNELS_TIME_SAMPLING_RATE` | Sampling rate for channel send/receive latency timings. (default: unset) |

## CPU Sampling

| Variable | Description |
|----------|-------------|
| `HOTPATH_SAMPLY_WRAPPER_BIN` | Path to the `hotpath-samply` wrapper binary that autospawn launches under the `hotpath-cpu` feature. (default: `hotpath-samply`, resolved via `PATH`) |
| `HOTPATH_SAMPLY_BIN` | Path to the external `samply` binary used by the `hotpath-samply` worker. (default: `samply`, resolved via `PATH`) |

## Metrics Server

| Variable | Description |
|----------|-------------|
| `HOTPATH_METRICS_PORT` | Port for the HTTP metrics server (binds to `localhost` only). (default: `6770`) |
| `HOTPATH_METRICS_SERVER_OFF` | Set to `true` or `1` to disable the HTTP metrics server entirely. (default: `false`) |

## MCP Server

| Variable | Description |
|----------|-------------|
| `HOTPATH_MCP_PORT` | Port for the MCP (Model Context Protocol) server. (default: `6771`) |
| `HOTPATH_MCP_AUTH_TOKEN` | When set, clients must include this token in the `Authorization` header. (default: `''`) |

## TUI

| Variable | Description |
|----------|-------------|
| `HOTPATH_TUI_REFRESH_INTERVAL_MS` | TUI dashboard refresh interval in milliseconds. (default: `500`) |
| `HOTPATH_TUI_TAB` | Initial tab to display when launching the TUI: `1` (Timing), `2` (Memory), `3` (Data Flow), `4` (Threads), `5` (Debug), `6` (Tokio). (default: unset) |
| `HOTPATH_TUI_AUTO_EXPAND_LOGS` | Auto-open the logs panel once initial data arrives and pin selection to the given table index. Set to an integer (e.g. `0` for the first row, `2` for the third). (default: unset) |
| `HOTPATH_METRICS_HOST` | Host URL that the TUI console connects to for metrics data. (default: `http://localhost`) |
| `HOTPATH_METRICS_PORT` | Port that the TUI console connects to for metrics data. (default: `6770`) |
| `HOTPATH_DISABLE_SAMPLY_LOAD` | Set to `true` or `1` to disable the `samply load` shortcut on the CPU subtab; the `'f'` keybinding and its hint are hidden. (default: `false`) |

## Other

| Variable | Description |
|----------|-------------|
| `HOTPATH_DRAIN_INTERVAL` | Interval in milliseconds between background worker sweeps of the per-thread event queues. Decrease for high-traffic apps to bound queue memory growth between sweeps, at the cost of more worker wakeups. (default: `50`) |
| `HOTPATH_THREADS_INTERVAL_MS` | Thread monitoring sample interval in milliseconds. (default: `250`) |
| `HOTPATH_TOKIO_RUNTIME_INTERVAL_MS` | Tokio runtime metrics sampling interval in milliseconds. (default: `1000`) |
| `HOTPATH_LOGS_LIMIT` | Maximum number of log entries to keep per channel, stream, or function. (default: `50`) |
| `HOTPATH_MAX_LOG_LEN` | Maximum character length for logged return values (`log = true`). Values exceeding this limit are truncated with `...`. (default: `1536`) |
| `HOTPATH_SHUTDOWN_MS` | If set a profiled program will shutdown after the specified ms timeout and print the performance report. (default: `''`). Use `before_shutdown` to specify before shutdown callback. |
