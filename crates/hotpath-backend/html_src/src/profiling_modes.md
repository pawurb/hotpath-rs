# Profiling modes: static reports and live monitoring

`hotpath` supports two complementary approaches to Rust performance profiling and runtime monitoring.

## Static reports

Best for CLI tools, tests, or short-lived applications. On exit, `hotpath` prints a summary of execution time, memory usage, and timing percentiles. Reports can be rendered as readable tables or exported as JSON for automated analysis.

Every instrumented program prints a performance report automatically when executed with the `hotpath` feature enabled.

```bash
cargo run --features=hotpath
```

<img loading="lazy" src="{{#asset-hash images/hotpath-timing-report.png}}" alt="hotpath-rs timing profiling report showing per-function execution statistics">

Use `--features='hotpath,hotpath-alloc'` to print memory usage report:

```bash
cargo run --features='hotpath,hotpath-alloc'
```

<img loading="lazy" src="{{#asset-hash images/hotpath-alloc-report.png}}" alt="hotpath-rs memory allocation profiling report showing per-function byte counts">

### Configuring static reports

| Variable | Description |
|----------|-------------|
| `HOTPATH_OUTPUT_FORMAT` | Output format: `table` (default), `json`, `json-pretty`, or `none`. Using `none` silences output while keeping the metrics server and MCP server active. |
| `HOTPATH_OUTPUT_PATH` | File path for profiling reports. Takes precedence over programmatic `output_path` config. Defaults to `stdout`. |
| `HOTPATH_REPORT` | Comma-separated sections to include: `functions-timing`, `functions-alloc`, `channels`, `streams`, `futures`, `threads`, `tokio_runtime`, or `all`. Defaults to `functions-timing,functions-alloc,threads`. |
| `HOTPATH_FOCUS` | Filter profiled functions by name. Plain text does substring matching; wrap in `/pattern/` for regex (e.g. `HOTPATH_FOCUS="/^(compute\|process)/"`). |
| `HOTPATH_METRICS_SERVER_OFF` | Set to `true` or `1` to disable the HTTP metrics server. Useful when you only need a static report and don't want to use a TUI. |

Example - write a JSON report containing only function timing and thread usage metrics to a file:

```bash
HOTPATH_OUTPUT_FORMAT=json \
HOTPATH_OUTPUT_PATH=report.json \
HOTPATH_REPORT=functions-timing,threads \
cargo run --features=hotpath
```

### Timed shutdown

`HOTPATH_SHUTDOWN_MS` forces the program to exit and print the report after a fixed duration. This is useful for profiling long-running processes (HTTP servers, workers) where you want to collect metrics for a predefined period without manual intervention. It also enables deterministic benchmarks - run the same workload for a fixed window across different git commits and compare the reports. Find more info on this technique in [A/B benchmarks](/benchmarks.md).

```bash
HOTPATH_SHUTDOWN_MS=10000 \
HOTPATH_OUTPUT_FORMAT=json \
HOTPATH_OUTPUT_PATH=tmp/report.json \
cargo run --features=hotpath
```

Use `before_shutdown` in the `HotpathGuardBuilder` API to run cleanup logic (flush connections, drain queues) before the report is generated.

## Live TUI dashboard

Best for long-running processes like HTTP servers, or background workers. It continuously displays function performance metrics, allocation counters, and channel/stream throughput while the application is running. This mode helps diagnose runtime bottlenecks, queue buildup, and data flow issues that are not visible in static summaries.

Install the TUI with:

```
cargo install hotpath --features=tui
```

Run the dashboard:

```
hotpath console
```

Then launch your instrumented application (with `hotpath` feature enabled) in a separate terminal to see live performance metrics.

<video loading="lazy" width="100%" loop muted playsinline controls poster="{{#asset-hash images/hotpath-live-dashboard-poster.jpg}}">
  <source src="{{#asset-hash videos/hotpath-live-dashboard.mp4}}" type="video/mp4">
</video>

You can learn how to instrument any Rust program in the next sections.
