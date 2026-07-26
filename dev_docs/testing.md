# Writing Integration Tests with JSON

Integration tests live in `crates/hotpath/tests/`. They spawn an example as a child process and assert on profiler output. There are two ways to read the metrics; prefer JSON over scraping human-readable table text. Pick based on whether you need live state or the exact terminal snapshot.

**Approach 1 - poll the metrics HTTP endpoint (live, mid-run state).** Spawn the example with `HOTPATH_METRICS_PORT` set and `TEST_SLEEP_SECONDS` to keep the process (and metrics server) alive, then `ureq::get("http://localhost:<port>/channels")` in a retry loop and `serde_json::from_str::<JsonChannelsList>` the body. Use this when asserting on metrics while the program is still running. Caveat: the endpoint reflects the worker's most recent sweep of the per-thread queues, so counts can transiently lag recent events until the next sweep - hence the retry loop. See `tests/channels_crossbeam.rs::test_data_endpoints`.

**Approach 2 - detect the JSON report printed at guard drop (exact terminal state).** When the assertion depends on the precise state captured when the guard is dropped (e.g. "50 messages parked, 0 received"), the endpoint may not be exact - rely on the report instead. Run the example with a guard configured for `Format::Json`/`Format::JsonPretty`, capture stdout, find the report's opening `{`, and read only the first JSON value with `serde_json::Deserializer::into_iter().next()` (the report is followed by trailing log lines, so a plain `from_str` on the whole stdout fails). Deserialize straight into the typed `hotpath::json::JsonReport` and read its fields (`report.channels`, `report.functions_timing`, ...) - do NOT go through `serde_json::Value` + `from_value(report["channels"])`.

```rust
use hotpath::json::{JsonChannelsList, JsonReport};

fn parse_channels(stdout: &str) -> JsonChannelsList {
    let json_start = stdout.find('{').expect("No JSON report in output");
    let report: JsonReport = serde_json::Deserializer::from_str(&stdout[json_start..])
        .into_iter::<JsonReport>()
        .next()
        .expect("No JSON value in output")
        .expect("Failed to parse JSON report");
    report.channels.expect("No channels section in report")
}
```

See `tests/channels_crossbeam_wrap.rs`.

Conventions: use a single module-level `#[cfg(all(test, feature = "hotpath"))]` guard per test file (not per-item annotations), and give each endpoint-polling test file its own `HOTPATH_METRICS_PORT` so parallel test files don't collide.

## Service-dependent tests (PostgreSQL, Redis)

Some tests need real services: `sql_pg.rs`, `diesel_pg.rs`, and `toasty_pg.rs` use PostgreSQL on `localhost:5439`; `io_redis.rs` uses Redis on `localhost:6390`. Both run from the repo-root compose file (non-default host ports so system-wide installs don't collide; postgres credentials are `hotpath`/`hotpath`, db `hotpath`):

```bash
cp docker-compose.yml.sample docker-compose.yml
docker compose up -d postgres redis
```

Each test probes its port first. Locally, when nothing listens there, the test prints a skip message and passes - so a green run does not prove they executed; start the containers to actually exercise them. On CI (`CI` env var set) the skip path panics instead, making the services mandatory. New tests that depend on an external service must follow the same pattern: probe the port, skip locally with a message, `assert!(std::env::var_os("CI").is_none(), ...)` on CI.
