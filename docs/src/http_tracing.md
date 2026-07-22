# Rust HTTP Client Performance Profiling for reqwest

`hotpath` profiles outgoing HTTP requests made with [reqwest](https://crates.io/crates/reqwest), helping you identify slow endpoints, frequent request patterns, and failing calls. Requests are grouped by normalized endpoint, so parameter-varied requests to the same route are reported together: 1,000 calls to `GET api.example.com/users/{id}` appear as a single entry with call count, error count, average latency, percentiles, and total execution time.

Instrumentation is inactive unless the `hotpath` feature is enabled.

## Wrapping the client

Add `hotpath` with the feature matching your `reqwest` crate version to your `Cargo.toml`:

```toml
[dependencies]
hotpath = "{{HOTPATH_VERSION}}", features=["reqwest-0-13"] # or "reqwest-0-12"
```

Wrap the client once at creation with the `http!` macro - every request sent through it is then profiled, with no changes at the call sites:

```rust
let client = hotpath::http!(reqwest::Client::new());

// Normal reqwest usage from here on:
let resp = client.get("https://api.example.com/users/1").send().await?;
```

Under the hood the macro wraps the client with [reqwest-middleware](https://crates.io/crates/reqwest-middleware)'s `ClientWithMiddleware` and attaches hotpath's timing middleware. The wrapped client mirrors the full reqwest API (`get`, `post`, `json`, `header`, ...), so existing request-building code compiles unchanged.

To store the client in a struct with a type that stays the same whether profiling is on or off, use the wrap alias:

```rust
struct App {
    client: hotpath::wrap::reqwest::Client,
}
```

With `hotpath` enabled this is `ClientWithMiddleware`; disabled, `http!` is a no-op and the alias is the raw `reqwest::Client`. Both reqwest generations can even be enabled at once (e.g. mid-migration); the unversioned `wrap::reqwest` alias then points at 0.13, and `wrap::reqwest_012` / `wrap::reqwest_013` name each generation explicitly.

### Labels

An optional `label` prefixes every endpoint key produced by the client - useful for telling apart clients talking to different services:

```rust
let github = hotpath::http!(reqwest::Client::new(), label = "github");
// -> "github: GET api.github.com/repos/{id}"
```

### Existing reqwest-middleware stacks

If your app already uses `reqwest-middleware` (e.g. for retries), skip the macro and attach the middleware directly:

```rust
let client = reqwest_middleware::ClientBuilder::new(reqwest::Client::new())
    .with(hotpath::ReqwestHttpMiddleware::new())
    .with(RetryTransientMiddleware::new_with_policy(retry_policy))
    .build();
```

Stack order matters: placed *before* a retry middleware, hotpath times the total including retries; placed *after* it, each attempt is timed separately.

## Normalizing endpoints

Requests are grouped by `METHOD host/path`. The query string, fragment, and credentials are dropped, and path segments that look like identifiers collapse into `{id}`:

- all-digit segments (`/users/123`)
- UUIDs (`/jobs/550e8400-e29b-41d4-a716-446655440000`)
- hex strings of 16+ characters (`/blobs/deadbeefdeadbeef`)

So `GET /users/1?verbose=true` and `GET /users/42` merge into one `GET api.example.com/users/{id}` bucket. Raw URLs never reach the report - only the normalized shape does.

## Error tracking

Each bucket has an `Errors` column counting transport errors (DNS failures, connection refused, timeouts) plus responses with status >= 400.

## Adding the HTTP section to your hotpath report

The `http` section is opt-in. Add it via the `HOTPATH_REPORT` env var (comma-separated `http`, or `all`), or programmatically through `HotpathGuardBuilder::sections`:

```rust
let _guard = hotpath::HotpathGuardBuilder::new("main")
    .sections(vec![hotpath::Section::Http])
    .build();
```

```bash
http - HTTP request execution time statistics.
+-------------------------------------+-------+--------+-----------+-----------+-----------+---------+
| Endpoint                            | Calls | Errors | Avg       | P95       | Total     | % Total |
+-------------------------------------+-------+--------+-----------+-----------+-----------+---------+
| GET api.example.com/users/{id}      | 120   | 0      | 781.35 µs | 1.21 ms   | 93.76 ms  | 58.99%  |
| GET api.example.com/search          | 40    | 2      | 301.79 µs | 350.12 µs | 12.07 ms  | 11.39%  |
+-------------------------------------+-------+--------+-----------+-----------+-----------+---------+
```

## Limiting and capping endpoint output

The number of endpoints shown is unlimited by default (`0`). Cap it with:

- Builder: `.http_limit(n)`
- Env var: `HOTPATH_HTTP_LIMIT`

## Live HTTP metrics

Live HTTP request metrics display in the `I/O -> HTTP` TUI tab, and are exposed as JSON at `GET /http` on the metrics server (per-endpoint request logs at `GET /http/{id}/logs`).

## What is measured

The middleware times reqwest's `execute` future, which resolves at a specific protocol moment: when the response **status line and headers have been fully received**. The `Response` returned at that point holds an open handle onto the connection - body bytes flow only when your code later awaits `.json()`, `.text()`, `.bytes()`, or polls `.bytes_stream()`, and by then the measurement is already recorded.

Included in the measured window:

- DNS resolution, TCP connect, and TLS handshake - but only when no pooled connection is reused; requests over a warm connection pool skip these
- *Sending* the request, including uploading the full request body
- Redirect hops (`execute` follows redirects internally, so each hop's full round trip counts)
- Server processing time, up to receipt of the response headers

Excluded:

- Downloading the response body, decompression, and JSON deserialization

Note the asymmetry: request-body *upload* is inside the window, response-body *download* is outside it.

In practice the metric behaves like **time-to-first-byte plus connection cost**. An endpoint that is slow because the server computes for 800ms before answering shows up accurately. An endpoint that answers headers in 20ms and then streams 50MB of JSON for 4 seconds shows up as 20ms. A body-read failure also won't appear in the `Errors` column - by then the request was already recorded with its response status. For SSE and long-poll endpoints that keep the body open indefinitely this is the behavior you want (the request would otherwise never "complete"); for large payload downloads it undercounts.

This boundary is inherent to the reqwest-middleware interception point - the `Middleware` chain hands back the `Response` as soon as headers arrive, and body consumption belongs to the caller. It captures server latency reliably; it is not a measure of network throughput.

## Other limitations

Only requests sent through wrapped clients are visible; HTTP calls made inside third-party crates holding their own `reqwest::Client` are not captured. The async client only - `reqwest::blocking` is not supported by `hotpath::http!`.
