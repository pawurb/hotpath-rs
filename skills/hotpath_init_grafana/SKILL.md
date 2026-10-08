---
name: hotpath_init_grafana
description: Set up a Grafana dashboard for a Rust project profiled with hotpath. Enables the hotpath-prometheus feature, configures the Prometheus scrape job and builds a dashboard JSON with context-aware panels (SQL and HTTP time per route, N+1 detection, memory and time per request, channel queues, lock contention, Tokio workers, per-thread CPU and allocations, profiling overhead) limited to what the project instruments. Use when the user wants Grafana or Prometheus dashboards for hotpath metrics.
allowed-tools: Bash, Read, Edit, Write, Glob, Grep
---

# Initialize a hotpath Grafana dashboard

Build a [Grafana](https://grafana.com) dashboard on top of the metrics that [hotpath](https://hotpath.rs) exposes through its Prometheus endpoint. The dashboard is organized in performance layers: SQL queries and outbound HTTP calls attributed to the axum route that issued them, memory and time per request, then channels, locks, the Tokio runtime and threads. Each panel answers a concrete question instead of plotting raw series.

This skill assumes Grafana and Prometheus are already running and connected. It does not install or operate them.

Reference: https://hotpath.rs/prometheus_grafana (every metric, label and env var), https://hotpath.rs/blog/rust-performance-grafana (the reasoning behind each panel).

## Steps

### 1. Inspect the project

- Check whether hotpath is already set up: a `hotpath` dependency, `#[hotpath::main]` (or `HotpathGuardBuilder`) and instrumented code. If it is not, set it up first by following the `hotpath_init` skill (https://raw.githubusercontent.com/pawurb/hotpath-rs/main/skills/hotpath_init/SKILL.md), then continue here.
- Record which subsystems the project instruments. They decide which dashboard rows to build:

| Instrumentation in the code | Dashboard row |
|---|---|
| `hotpath::axum!(router)` | Routes, and every per-route panel |
| sqlx / diesel / toasty SQL layer | SQL |
| `hotpath::http!(client)` | Outbound HTTP |
| `hotpath-alloc` feature | Memory |
| `#[hotpath::measure]` / `#[hotpath::measure_all]` | Functions, profiling overhead |
| `hotpath::channel!` | Channels |
| `hotpath::rw_lock!` / `hotpath::mutex!` | Locks |
| `hotpath::tokio_runtime!()` | Tokio runtime |
| `threads` feature (on by default) | Threads |

- Skip any row whose instrumentation is missing; an empty panel is noise. Mention the skipped rows to the user with the macro that would enable them, but do not add instrumentation unless they ask.

### 2. Enable the Prometheus endpoint

In the crate's `Cargo.toml`, next to the existing hotpath features (never in `default`):

```toml
[features]
hotpath = ["hotpath/hotpath"]
hotpath-alloc = ["hotpath/hotpath-alloc"]
hotpath-prometheus = ["hotpath/hotpath-prometheus"]
```

Start the app with the feature **in the background** (a server never exits, so a foreground `cargo run` would block the check), give it some traffic, scrape the endpoint, then stop it:

```bash
cargo run --features='hotpath,hotpath-alloc,hotpath-prometheus' > /tmp/hotpath-app.log 2>&1 &
APP_PID=$!
# wait until the endpoint answers (the first build can take minutes), exercise the app, then:
curl -s http://127.0.0.1:6772/metrics | grep -v '^#' | cut -d'{' -f1 | sort -u
kill $APP_PID
```

The `curl` lists the metric families the app actually emits; families with no data are omitted from the scrape. Use that list to confirm step 1. If the app cannot be started locally (it needs a database, secrets, ...), ask the user to run it or rely on the code inspection alone.

The endpoint listens on `127.0.0.1:6772`. Configure it with env vars, not code:

- `HOTPATH_PROMETHEUS_PORT` / `HOTPATH_PROMETHEUS_HOST` - set the host to `0.0.0.0` when Prometheus runs in a container or on another machine.
- `HOTPATH_PROMETHEUS_AUTH_TOKEN` - required in the `Authorization` header (bare or `Bearer`-prefixed). Recommend it whenever the endpoint is reachable beyond localhost.

### 3. Configure the Prometheus scrape job

Find the existing Prometheus config (`prometheus.yml`, a docker-compose service, a Helm values file, ...) and ask the user where it lives when it is not in the repository. Add a job:

```yaml
scrape_configs:
  - job_name: hotpath
    scrape_native_histograms: true
    # authorization:
    #   credentials: <HOTPATH_PROMETHEUS_AUTH_TOKEN>
    static_configs:
      - targets: ["127.0.0.1:6772"]
```

Every query below assumes **native histograms**: `histogram_sum()`, `histogram_count()` and `histogram_quantile()` on the bare metric name. Prometheus 3.x accepts `scrape_native_histograms: true` per job; Prometheus 2.x needs the `--enable-feature=native-histograms` flag instead. If the user cannot enable native histograms, rewrite every histogram query to the classic form: `histogram_quantile(q, sum by (x, le) (increase(<metric>_bucket[...])))`, `histogram_sum(increase(<metric>[...]))` becomes `increase(<metric>_sum[...])` and `histogram_count(...)` becomes `increase(<metric>_count[...])`.

Never restart or reload a running Prometheus without asking.

### 4. Build the dashboard

Write the dashboard as Grafana JSON to `grafana/hotpath-dashboard.json` (or next to the project's existing dashboards). Requirements:

- A `datasource` template variable of type `prometheus`, used by every panel and every query variable, so the file imports into any Grafana.
- A `route` query variable, `label_values(hotpath_server_requests_total, route)`, multi-value with an "All" option (`.*`), only when the routes row exists.
- Textbox variables `overhead_ns` (default `50`), `threshold` (default `1`) and `min_share` (default `1`) for the profiling overhead panel.
- One collapsible row per layer from step 1, in the order below.
- Ranking panels ("top 10 ...") use **instant** queries over `$__range`, rendered as horizontal bar gauges, with the legend set to the grouping label (e.g. `{{query}}`, `{{route}}`). Set the unit per panel: `percent`, `s`, `bytes`, `short`.
- Long label values (SQL text, function paths) need a wide panel and value truncation off, or a table panel.

#### SQL

Share of total SQL time per query:

```promql
sort_desc(topk(10,
  100 * sum by (query) (histogram_sum(increase(hotpath_sql_duration_seconds[$__range])))
  / on() group_left()
  sum(histogram_sum(increase(hotpath_sql_duration_seconds[$__range])))
))
```

Slow queries, p95 above 500ms:

```promql
sort_desc(histogram_quantile(0.95, sum by (query) (increase(hotpath_sql_duration_seconds[$__range]))) > 0.500)
```

#### Routes (axum)

Share of total response time per route:

```promql
sort_desc(topk(10,
  100 * sum by (route) (histogram_sum(increase(hotpath_server_duration_seconds[$__range])))
  / on() group_left()
  sum(histogram_sum(increase(hotpath_server_duration_seconds[$__range])))
))
```

Slowest routes by p95:

```promql
sort_desc(topk(10, histogram_quantile(0.95, sum by (route) (increase(hotpath_server_duration_seconds[$__range])))))
```

SQL time per request, by query, for the selected `$route` (only with the SQL row):

```promql
sort_desc(topk(10,
  sum by (query) (histogram_sum(increase(hotpath_sql_duration_seconds{route=~"$route"}[$__range])))
  / scalar(sum(increase(hotpath_server_scoped_requests_total{route=~"$route"}[$__range])))
))
```

SQL queries per request, the N+1 detector (only with the SQL row):

```promql
sort_desc(topk(10,
  sum by (route) (increase(hotpath_server_sql_calls_total[$__range]))
  / (sum by (route) (increase(hotpath_server_scoped_requests_total[$__range])) > 0)
))
```

#### Outbound HTTP

HTTP calls per request (needs the routes row):

```promql
sort_desc(topk(10,
  (sum by (route) (increase(hotpath_server_http_calls_total[$__range])) > 0)
  / (sum by (route) (increase(hotpath_server_scoped_requests_total[$__range])) > 0)
))
```

HTTP time per request; `route!=""` drops calls made outside a request, e.g. background jobs:

```promql
sort_desc(topk(10,
  sum by (route) (histogram_sum(increase(hotpath_http_duration_seconds{route!=""}[$__range])))
  / (sum by (route) (increase(hotpath_server_scoped_requests_total[$__range])) > 0)
))
```

Without axum, rank endpoints by total time instead: `sort_desc(topk(10, sum by (endpoint) (histogram_sum(increase(hotpath_http_duration_seconds[$__range])))))`.

#### Memory (`hotpath-alloc`)

Bytes allocated per request, by route:

```promql
sort_desc(topk(10,
  sum by (route) (increase(hotpath_server_alloc_bytes_total[$__range]))
  / (sum by (route) (increase(hotpath_server_scoped_requests_total[$__range])) > 0)
))
```

Bytes allocated per request, by function, for the selected `$route`. Allocations are exclusive by default: each function reports only what it allocates itself.

```promql
sort_desc(topk(10,
  sum by (function) (increase(hotpath_function_route_alloc_bytes_total{route=~"$route"}[$__range]))
  / scalar(sum(increase(hotpath_server_scoped_requests_total{route=~"$route"}[$__range])))
))
```

Without axum, use `sort_desc(topk(10, sum by (function) (increase(hotpath_function_alloc_bytes_total[$__range]))))`.

#### Functions

Time per request, by function, for the selected `$route`. Dividing by timed calls and multiplying by calls keeps it correct under time sampling. The time is inclusive of nested instrumented calls.

```promql
sort_desc(topk(10,
  sum by (function) (increase(hotpath_function_route_duration_seconds_total{route=~"$route"}[$__range]))
  / (sum by (function) (increase(hotpath_function_route_timed_calls_total{route=~"$route"}[$__range])) > 0)
  * sum by (function) (increase(hotpath_function_route_calls_total{route=~"$route"}[$__range]))
  / scalar(sum(increase(hotpath_server_scoped_requests_total{route=~"$route"}[$__range])))
))
```

Without axum, share of total instrumented time per function:

```promql
sort_desc(topk(10,
  100 * sum by (function) (histogram_sum(increase(hotpath_function_duration_seconds[$__range])))
  / on() group_left()
  sum(histogram_sum(increase(hotpath_function_duration_seconds[$__range])))
))
```

#### Channels

Every channel query groups by `source`, `label` and `iter`, the labels that identify one channel: `label` is empty for channels created without one, so grouping by it alone would merge unrelated call sites. Use `{{label}} {{source}}` as the legend.

Max queue size since start; the capacity is in the `type` label. A queue at capacity means consumers are not keeping up.

```promql
sort_desc(topk(10, max by (source, label, iter, type) (hotpath_channel_max_queue_size)))
```

Throughput, messages received per second:

```promql
sort_desc(topk(10, sum by (source, label, iter, type) (rate(hotpath_channel_received_total[$__range]))))
```

p95 send-to-receive latency:

```promql
sort_desc(topk(10, histogram_quantile(0.95, sum by (source, label, iter, type) (increase(hotpath_channel_delay_seconds[$__range])))))
```

#### Locks

p95 wait time (contention) and p95 hold time per RwLock and side, grouped by the call-site labels like the channels (legend `{{label}} {{source}} {{op}}`):

```promql
sort_desc(topk(10, histogram_quantile(0.95, sum by (source, label, iter, op) (increase(hotpath_rwlock_wait_seconds[$__range])))))
sort_desc(topk(10, histogram_quantile(0.95, sum by (source, label, iter, op) (increase(hotpath_rwlock_acquire_seconds[$__range])))))
```

For mutexes use `hotpath_mutex_wait_seconds` and `hotpath_mutex_acquire_seconds`, grouped by `(source, label, iter)`.

#### Tokio runtime

One table panel with one instant query per column, each in table format grouped by `worker`, merged with the "Merge" transformation:

```promql
sum by (worker) (increase(hotpath_tokio_worker_parks_total[$__range]))
sum by (worker) (increase(hotpath_tokio_worker_polls_total[$__range]))
sum by (worker) (increase(hotpath_tokio_worker_steals_total[$__range]))
sum by (worker) (increase(hotpath_tokio_worker_busy_seconds_total[$__range]))
sum by (worker) (rate(hotpath_tokio_worker_busy_seconds_total[$__range]))
sum by (worker) (hotpath_tokio_worker_local_queue_depth)
sum by (worker) (increase(hotpath_tokio_worker_busy_seconds_total[$__range]))
  / (sum by (worker) (increase(hotpath_tokio_worker_polls_total[$__range])) > 0)
```

Columns: Parks, Polls, Steals, Busy, Busy %, Local queue, Mean poll. Some of these need `RUSTFLAGS="--cfg tokio_unstable"`; tell the user if the family is missing from the scrape.

#### Threads

Peak CPU per thread, and total bytes allocated per thread (`hotpath-alloc`):

```promql
sort_desc(topk(10, hotpath_thread_cpu_percent_max))
sort_desc(topk(10, hotpath_thread_alloc_bytes_total))
```

Retained bytes per thread (allocated minus deallocated since start), as a time series; a line that keeps rising under steady load points to a thread holding memory:

```promql
hotpath_thread_alloc_bytes_total - hotpath_thread_dealloc_bytes_total
```

Memory freed on another thread than the one that allocated it (e.g. buffers handed through a channel) makes one thread drift up and the other down, so read a rising line together with the process-level `hotpath_alloc_bytes_total - hotpath_dealloc_bytes_total`.

Legend `{{name}}`. Unnamed threads show up as `thread_N`; suggest `std::thread::Builder::name` for the important ones.

#### Profiling overhead

Functions where the per-call profiling cost (`$overhead_ns`) exceeds `$threshold` percent of the average call, limited to functions above `$min_share` percent of total instrumented time. An empty panel is the good outcome.

```promql
sort_desc(
  (
    100 * ($overhead_ns * 1e-9) / (
      sum by (function) (histogram_sum(increase(hotpath_function_duration_seconds[$__range])))
      / (sum by (function) (histogram_count(increase(hotpath_function_duration_seconds[$__range]))) > 0)
    )
    > $threshold
  )
  and on (function)
  (
    100 * sum by (function) (histogram_sum(increase(hotpath_function_duration_seconds[$__range])))
    / scalar(sum(histogram_sum(increase(hotpath_function_duration_seconds[$__range]))))
    > $min_share
  )
)
```

Functions listed here are candidates for removing `#[hotpath::measure]` or enabling time sampling (https://hotpath.rs/profiling_overhead).

### 5. Validate the queries

When a Prometheus URL is reachable and already scrapes the app, run each panel query through the HTTP API with `$__range` replaced by a concrete window (e.g. `1h`) and the variables by their defaults, and fix any query that returns an error:

```bash
curl -sG http://127.0.0.1:9090/api/v1/query --data-urlencode 'query=<promql>'
```

An empty `result` is fine when the app has not served that kind of traffic yet; a `"status":"error"` is not. Check the JSON itself with `jq . grafana/hotpath-dashboard.json`.

### 6. Import the dashboard

Ask the user how they want it imported:

- **Manually**: Grafana UI, Dashboards, New, Import, upload `grafana/hotpath-dashboard.json`, pick the Prometheus data source.
- **Through the API**: with a Grafana URL and a service account token (Editor role) from the user, wrap the dashboard and post it. Never write the token to a file in the repository.

```bash
jq '{dashboard: (. + {id: null}), overwrite: true}' grafana/hotpath-dashboard.json \
  | curl -sS -X POST "$GRAFANA_URL/api/dashboards/db" \
      -H "Authorization: Bearer $GRAFANA_TOKEN" -H 'Content-Type: application/json' --data-binary @-
```

- **Provisioning**: if the project already provisions Grafana dashboards from files, put the JSON where the existing provider reads it.

### 7. Summarize

Tell the user:

- which rows the dashboard contains, and which were skipped with the instrumentation that would enable them;
- the cargo features and env vars the app needs in production (`hotpath,hotpath-prometheus`, plus `hotpath-alloc` for the memory row) and that the endpoint should be protected with `HOTPATH_PROMETHEUS_AUTH_TOKEN` or kept on localhost;
- how to read the first panels to look at: SQL queries per request above ~5 is an N+1 candidate, a channel at max capacity is a bottleneck, a non-empty profiling overhead panel means some instrumentation costs more than it is worth.
