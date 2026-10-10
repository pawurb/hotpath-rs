# Writing Integration Tests with JSON

Integration tests live in `crates/hotpath/tests/`. They spawn an example as a child process and assert on its JSON output, never on table text.

**Live state: poll the metrics endpoint.** Run the example with `HOTPATH_METRICS_PORT` and `TEST_SLEEP_SECONDS`, then fetch the route in a retry loop (counts lag until the worker's next sweep). See `tests/channels_crossbeam.rs::test_data_endpoints`.

**Exact final state: parse the guard-drop report.** Run with `Format::Json` and parse stdout into the typed `JsonReport`, not `serde_json::Value`; log lines follow the report. See `parse_channels` in `tests/channels_crossbeam.rs`.

Conventions: one module-level `#[cfg(all(test, feature = "hotpath"))]` guard per test file, and a unique `HOTPATH_METRICS_PORT` per endpoint-polling test file.

## Service-dependent tests (PostgreSQL, Redis)

`sql_pg.rs`, `diesel_pg.rs` and `toasty_pg.rs` need PostgreSQL on `localhost:5439`, `io_redis.rs` needs Redis on `localhost:6390` (see `docker-compose.yml.sample`):

```bash
cp docker-compose.yml.sample docker-compose.yml
docker compose up -d postgres redis
```

Locally these tests skip and pass when the port is closed, so a green run does not prove they ran; on CI (`CI` set) the skip panics. New service-dependent tests must follow the same probe-skip-panic pattern.
