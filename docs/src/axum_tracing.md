# Rust HTTP Server Performance Profiling for axum

`hotpath` profiles the requests your [axum](https://crates.io/crates/axum) application serves, reporting response time per route so you can see which endpoints are slow, which are hit most, and which return errors. Requests are grouped by the route template that handled them - 1,000 requests to `GET /users/{id}` appear as a single entry with request count, 4xx/5xx counts, SQL queries and outbound HTTP requests per request, average latency, percentiles, and total time.

## Wrapping the router

Add `hotpath` with the `axum-0-8` feature to your `Cargo.toml`:

```toml
[dependencies]
hotpath = { version = "{{HOTPATH_VERSION}}", features = ["axum-0-8"] }
```

Wrap the finished router with the `axum!` macro - every request it serves is then profiled, with no other code changes required:

```rust
use axum::{routing::{get, post}, Router};

let app = hotpath::axum!(Router::new()
    .route("/users/{id}", get(get_user))
    .route("/users", post(create_user)));

axum::serve(listener, app).await?;
```

Under the hood the macro expands to `router.layer(hotpath::AxumLayer::new())`, a tower layer that records the request until its response head is produced. Because `Router::layer` only applies to routes that already exist, the macro must wrap the router *after* the last `.route(..)` / `.fallback(..)` call - the same rule as `TraceLayer` and other tower middleware. Routes added later are not profiled.

Apply the layer once, to the outermost router: it already sees the routes of nested routers (`Router::nest`) through `MatchedPath`. If a request passes through a second `AxumLayer` - a nested router that was wrapped separately, or a sub-request dispatched into another wrapped router - the inner layer detects the outer request scope and stays silent, so the request is reported once with the outer layer's route; hotpath prints a warning to stderr the first time this happens.

With the `hotpath` feature disabled the macro returns the router unchanged and `AxumLayer` is a pass-through, so the wrapping line can stay in place unconditionally.

### Existing middleware stacks

To control where hotpath sits relative to your other layers, skip the macro and add the layer yourself:

```rust
let app = Router::new()
    .route("/users/{id}", get(get_user))
    .layer(hotpath::AxumLayer::new())
    .layer(TraceLayer::new_for_http());
```

Layer order matters: middleware added later runs *outside* middleware added earlier. Placed innermost (first), hotpath times only the handler; placed outermost (last), it times the whole stack including authentication, compression, and other layers.

## Route bucketing

Requests are keyed by `METHOD template`, where the template is the axum route pattern that matched (`axum::extract::MatchedPath`), so `GET /users/1?verbose=true` and `GET /users/42` both land in `GET /users/{id}`. Nested routers report the full path including the nest prefix. Query strings and raw path parameters never reach the report.

Requests that match no route - the router's fallback, or services mounted with `nest_service` - carry no `MatchedPath`. What happens next depends on the response status:

- Unmatched requests that end in an error status (>= 400) collapse into a single per-method `<unmatched>` bucket (`GET <unmatched>`). Internet scanners probing `GET /.env`, `/wp-login.php` and friends are by definition unmatched and rejected, so their volume stays visible as one row instead of one row per probed path.
- Unmatched requests served successfully (a fallback `ServeDir`, a `nest_service` target like `/blog/first-post`) are bucketed by their raw path with id-like segments (all-digit, UUID, 16+ hex chars) collapsed to `{id}`, the same normalization used for [outgoing HTTP requests](http_tracing.md#normalizing-endpoints), so per-page stats are kept while cardinality stays bounded.

## Error tracking

Each route has `4xx` and `5xx` columns counting responses by status class. They are split because 4xx responses are usually the client's fault (validation errors, missing resources) while 5xx responses point at the handler.

## Route scoping for SQL and HTTP

When the layer is installed, [SQL queries](sql_tracing.md) and [outbound HTTP requests](http_tracing.md) issued while a handler runs are additionally attributed to the route that triggered them. Both sections gain a `Route` column (and a `route` field in the JSON report, metrics API, and MCP tools) next to the existing `Source` column, shown only when at least one entry has a route:

```
sql - SQL query execution time statistics.
+-----------------------------------------+----------------+--------------------+-------+----------+
| Query                                   | Source         | Route              | Calls | Avg      |
+-----------------------------------------+----------------+--------------------+-------+----------+
| SELECT id, name FROM users WHERE id = ? | app::load_user | GET /users/{id}    | 5     | 31.83 µs |
| SELECT id, name FROM users WHERE id = ? | app::load_user | GET /profiles/{id} | 3     | 32.14 µs |
| INSERT INTO users (name) VALUES (?)     | -              | -                  | 3     | 28.75 µs |
+-----------------------------------------+----------------+--------------------+-------+----------+
```

The route is part of the grouping key, so the same statement (or the same outbound endpoint) executed under two routes appears as two rows. `Source` and `Route` are independent: a query from uninstrumented code inside a handler gets a route but no source, and a query outside any request gets neither.

Route attribution is on by default. Turn it off with `.route_scope(false)` on the `HotpathGuardBuilder`, or with `HOTPATH_ROUTE_SCOPE=0`, which takes precedence.

### Queries and requests per route

The `server` section turns that attribution into per-route averages: `SQL/req` is the number of SQL queries issued per request of the route, `HTTP/req` the number of outbound HTTP requests. Each column appears only when the corresponding profiling subsystem is active. This is how N+1 query patterns surface - a list endpoint averaging 51 queries per request is loading its rows one by one:

```
server - HTTP server response time statistics per route.
+--------------------+-------+-----+-----+---------+----------+-----------+-----------+
| Route              | Calls | 4xx | 5xx | SQL/req | HTTP/req | Avg       | P95       |
+--------------------+-------+-----+-----+---------+----------+-----------+-----------+
| GET /profiles/{id} | 3     | 0   | 0   | 2.0     | 1.0      | 530.60 µs | 830.46 µs |
| GET /users/{id}    | 5     | 0   | 0   | 1.0     | 0.0      | 49.41 µs  | 92.09 µs  |
| GET <unmatched>    | 1     | 1   | 0   | -       | -        | 4.54 µs   | 4.54 µs   |
+--------------------+-------+-----+-----+---------+----------+-----------+-----------+
```

Each request counts the queries and outbound requests issued under its route scope and reports them when its response head is produced, so the averages describe exactly the completed requests in `Calls` - requests still in flight contribute nothing until they finish, and the live `/server` endpoint stays consistent with the SQL and HTTP sections. The values are exposed as `sql_per_request` / `http_per_request` in the JSON report, the `/server` metrics endpoint, and MCP tools, and shown in the TUI server panel. They inherit the limits of route scoping:

- `-` means no completed request of the route carried a route scope: requests that match no route (fallback, `nest_service`), route scoping disabled, or templates beyond the `HOTPATH_ENTRIES_LIMIT` route cap. A matched route whose handler issues no queries shows `0.0`.
- Queries that run inside `tokio::spawn` / `spawn_blocking` or on a driver worker thread are outside the route scope and are not counted, same as for the `Route` column.
- Only the average is reported, so a route whose requests alternate between 1 and 100 queries shows `50.5`.

### Memory per route

With the [`hotpath-alloc`](profiling_modes.md) feature the `server` section gains a second table: how much memory each route's requests allocate. Every request that carries a route scope counts the bytes and the number of allocations made while its future is polled - extractors, `Json` deserialization, the handler body, tower layers below the hotpath layer, response serialization - not only the code inside `#[hotpath::measure]` functions, and the total is inclusive of any measured functions the handler calls. The table lists routes in the same order as the response-time table:

```
server - HTTP server response time statistics per route.
+-----------------+-------+-----+-----+-----------+-----------+-----------+---------+
| Route           | Calls | 4xx | 5xx | Avg       | P95       | Total     | % Total |
+-----------------+-------+-----+-----+-----------+-----------+-----------+---------+
| GET /big        | 3     | 0   | 0   | 122.47 µs | 232.70 µs | 367.42 µs | 93.59%  |
| GET /small      | 4     | 0   | 0   | 5.60 µs   | 8.46 µs   | 22.41 µs  | 5.71%   |
| GET <unmatched> | 1     | 1   | 0   | 2.75 µs   | 2.75 µs   | 2.75 µs   | 0.70%   |
+-----------------+-------+-----+-----+-----------+-----------+-----------+---------+

+-----------------+-------+------------+---------+---------+---------+---------+
| Route           | Calls | Allocs/req | Avg     | P95     | Total   | % Total |
+-----------------+-------+------------+---------+---------+---------+---------+
| GET /big        | 3     | 6.0        | 1.0 MB  | 1.0 MB  | 3.0 MB  | 99.91%  |
| GET /small      | 4     | 5.0        | 714 B   | 714 B   | 2.8 KB  | 0.09%   |
| GET <unmatched> | 1     | -          | -       | -       | -       | -       |
+-----------------+-------+------------+---------+---------+---------+---------+
```

`Allocs/req` is the average number of allocations per request, `Avg` and the percentiles are bytes allocated per request (from a per-route histogram, so `P95` is a real percentile, not an average), `Total` the bytes allocated by all of the route's requests and `% Total` its share of every route's total. The JSON report, the `/server` metrics endpoint and MCP tools expose the same numbers as an `alloc` object on each server entry (`bytes_per_request`, `allocs_per_request`, `total_bytes`, `avg`, `total`, `percent_total`, `percentiles`) plus `total_alloc_bytes` on the list; the TUI server panel shows the memory table under the response-time table, and the Prometheus exporter adds `hotpath_server_alloc_bytes_total`, `hotpath_server_alloc_count_total` and the `hotpath_server_alloc_bytes` histogram per `route`.

What the numbers mean:

- **Bytes allocated**, not bytes retained: deallocations are not subtracted and peak RSS is not involved, the same semantics as the [functions allocation report](profiling_modes.md). A request that builds and drops a 1 MiB buffer reports 1 MiB.
- Only work polled inside the layer's future counts. Work moved to `tokio::spawn` or `spawn_blocking`, or done on a driver's worker thread, is not attributed to the request - the same hole as for function-level allocation profiling.
- The scope closes when the response head is produced, so a streaming body serialized later by hyper is outside it, matching the route's response time.
- Layers attached *after* `AxumLayer` in the stack (outside it) are not counted; layers below it are.
- `-` means no completed request of the route carried a route scope (unmatched routes, `HOTPATH_ROUTE_SCOPE=0`, the route interner cap), exactly as for `SQL/req`.

To see *which* functions a route spends its time and memory in, the [Prometheus exporter](prometheus_grafana.md#functions) also splits every measured function's calls, time and (with `hotpath-alloc`) bytes by the route they ran under: `hotpath_function_route_calls_total`, `hotpath_function_route_duration_seconds_total`, `hotpath_function_route_alloc_bytes_total` and friends, all labelled `function` + `route`. The text report and JSON keep one entry per function.

## Limiting and capping route output

The number of routes shown is unlimited by default (`0`). Cap it with:

- Builder: `.server_limit(n)`
- Env var: `HOTPATH_SERVER_LIMIT`

## What is measured

The layer times the request from the moment it enters the middleware until the inner service produces the response - the status line and headers. Everything a handler does before returning is inside the window: extractors, database queries, outbound HTTP calls, serialization of an in-memory body. Streaming a response body afterwards is not: for `Body::from_stream`, SSE, and long-poll endpoints the measurement covers the time to the response head, not the lifetime of the connection. This mirrors the [client-side HTTP measurement](http_tracing.md#what-is-measured), which stops when response headers arrive.

## Other limitations

Only requests that pass through the wrapped router are visible; work moved off the request future with `tokio::spawn` or `spawn_blocking` still counts towards the request only if the handler awaits it before responding. Only axum 0.8 is supported.
