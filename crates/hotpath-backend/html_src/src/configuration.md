# Environment Variables Configuration

`hotpath` behavior can be customized via environment variables. These take precedence over programmatic configuration (`hotpath::main` macro parameters and builder API).

## Output

| Variable | Description |
|----------|-------------|
| `HOTPATH_OUTPUT_FORMAT` | Output format: `table`, `json`, `json-pretty`, or `none`. Using `none` silences output while keeping the metrics server and MCP server active. (default: `table`) |
| `HOTPATH_OUTPUT_PATH` | File path for profiling reports. Takes precedence over programmatic `output_path` config. (default: `stdout`) |
| `HOTPATH_REPORT` | Comma-separated sections to include in report: `functions-timing`, `functions-alloc`, `channels`, `streams`, `futures`, `threads`, `tokio_runtime`, or `all`. (default: `functions-timing,functions-alloc,threads`) |

## Functions

| Variable | Description |
|----------|-------------|
| `HOTPATH_FOCUS` | Filter profiled functions by name. Plain text does substring matching; wrap in `/pattern/` for regex (e.g. `HOTPATH_FOCUS="/^(compute\|process)/"`). (default: `''`) |
| `HOTPATH_EXCLUDE_WRAPPER` | Set to `true` or `1` to calculate ratios using the sum of measured functions instead of the wrapper total. (default: `false`) |
| `HOTPATH_ALLOC_SELF` | Set to `true` or `1` to track exclusive (non-cumulative) memory allocations per function instead of the default cumulative mode. (default: `false`) |
| `HOTPATH_ALLOC_METRIC` | Primary metric for alloc mode: `bytes` or `count`. Controls sorting, percentages, and displayed values in reports. (default: `bytes`) |
| `HOTPATH_CPU_BASELINE_OFF` | Set to `true` or `1` to disable CPU baseline collection. (default: `false`) |

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
| `HOTPATH_TUI_TAB` | Initial tab to display when launching the TUI (e.g. `functions`, `channels`, `streams`). Useful for self-benchmarks. (default: `''`) |
| `HOTPATH_METRICS_HOST` | Host URL that the TUI console connects to for metrics data. (default: `http://localhost`) |
| `HOTPATH_METRICS_PORT` | Port that the TUI console connects to for metrics data. (default: `6770`) |

## Other

| Variable | Description |
|----------|-------------|
| `HOTPATH_THREADS_INTERVAL_MS` | Thread monitoring sample interval in milliseconds. (default: `1000`) |
| `HOTPATH_TOKIO_RUNTIME_INTERVAL_MS` | Tokio runtime metrics sampling interval in milliseconds. (default: `1000`) |
| `HOTPATH_LOGS_LIMIT` | Maximum number of log entries to keep per channel, stream, or function. (default: `50`) |
| `HOTPATH_SHUTDOWN_MS` | If set a profiled program will shutdown after the specified ms timeout and print the performance report. (default: `''`). Use `before_shutdown` to specify before shutdown callback. |
