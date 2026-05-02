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
| `HOTPATH_FUNCTIONS_NAME_DEPTH` | Number of module segments to keep when displaying function names (including the function name itself). `1` = function name only, `2` = one module + function, `0` = unlimited (full path). When using the TUI, set this env var for the TUI process too, since the console applies name shortening in its own process. (default: `2`) |

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

## Other

| Variable | Description |
|----------|-------------|
| `HOTPATH_THREADS_INTERVAL_MS` | Thread monitoring sample interval in milliseconds. (default: `250`) |
| `HOTPATH_TOKIO_RUNTIME_INTERVAL_MS` | Tokio runtime metrics sampling interval in milliseconds. (default: `1000`) |
| `HOTPATH_LOGS_LIMIT` | Maximum number of log entries to keep per channel, stream, or function. (default: `50`) |
| `HOTPATH_MAX_LOG_LEN` | Maximum character length for logged return values (`log = true`). Values exceeding this limit are truncated with `...`. (default: `1536`) |
| `HOTPATH_SHUTDOWN_MS` | If set a profiled program will shutdown after the specified ms timeout and print the performance report. (default: `''`). Use `before_shutdown` to specify before shutdown callback. |
