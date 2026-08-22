# Rust HTTP Server Performance Profiling for axum

`hotpath` profiles the requests your [axum](https://crates.io/crates/axum) application serves, reporting response time per route so you can see which endpoints are slow, which are hit most, and which return errors. Requests are grouped by the route template that handled them - 1,000 requests to `GET /users/{id}` appear as a single entry with request count, 4xx/5xx counts, average latency, percentiles, and total time.

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

Requests that match no route - the router's fallback, or services mounted with `nest_service` - carry no `MatchedPath`. Those are bucketed by their raw path with id-like segments (all-digit, UUID, 16+ hex chars) collapsed to `{id}`, the same normalization used for [outgoing HTTP requests](http_tracing.md#normalizing-endpoints), so cardinality stays bounded.

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

The route is part of the grouping key, so the same statement (or the same outbound endpoint) executed under two routes appears as two rows. Dividing a row's call count by the route's request count in the `server` section gives the number of queries per request for that route, which is how N+1 query patterns surface. `Source` and `Route` are independent: a query from uninstrumented code inside a handler gets a route but no source, and a query outside any request gets neither.

The layer sets the route around every poll of the handler future, the same way `Source` is tracked, so concurrent requests interleaved on one runtime thread never see each other's route. The same limits apply: work moved off the request future with `tokio::spawn` or `spawn_blocking` runs outside the route context, and async sqlx sqlite executes statements on its own connection worker thread where neither source nor route is visible (PostgreSQL/MySQL sqlx drivers, Diesel, and Toasty run on the calling task and are attributed normally). Requests that match no route (fallback, `nest_service`) set no route context.

Route scoping is on whenever the layer is installed. Turn it off with `HotpathGuardBuilder::route_scope(false)` or `HOTPATH_ROUTE_SCOPE=0`, which collapses the SQL and HTTP sections back to `(source, query)` grouping.

## Limiting and capping route output

The number of routes shown is unlimited by default (`0`). Cap it with:

- Builder: `.server_limit(n)`
- Env var: `HOTPATH_SERVER_LIMIT`

## What is measured

The layer times the request from the moment it enters the middleware until the inner service produces the response - the status line and headers. Everything a handler does before returning is inside the window: extractors, database queries, outbound HTTP calls, serialization of an in-memory body. Streaming a response body afterwards is not: for `Body::from_stream`, SSE, and long-poll endpoints the measurement covers the time to the response head, not the lifetime of the connection. This mirrors the [client-side HTTP measurement](http_tracing.md#what-is-measured), which stops when response headers arrive.

## Other limitations

Only requests that pass through the wrapped router are visible; work moved off the request future with `tokio::spawn` or `spawn_blocking` still counts towards the request only if the handler awaits it before responding. Per-request memory allocations are not tracked. Only axum 0.8 is supported.
