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

## Limiting and capping route output

The number of routes shown is unlimited by default (`0`). Cap it with:

- Builder: `.server_limit(n)`
- Env var: `HOTPATH_SERVER_LIMIT`

## What is measured

The layer times the request from the moment it enters the middleware until the inner service produces the response - the status line and headers. Everything a handler does before returning is inside the window: extractors, database queries, outbound HTTP calls, serialization of an in-memory body. Streaming a response body afterwards is not: for `Body::from_stream`, SSE, and long-poll endpoints the measurement covers the time to the response head, not the lifetime of the connection. This mirrors the [client-side HTTP measurement](http_tracing.md#what-is-measured), which stops when response headers arrive.

## Other limitations

Only requests that pass through the wrapped router are visible; work moved off the request future with `tokio::spawn` or `spawn_blocking` still counts towards the request only if the handler awaits it before responding. Per-request memory allocations are not tracked. Only axum 0.8 is supported.
