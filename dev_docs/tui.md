# Live TUI Console Reference

Terminal-based TUI for real-time monitoring. See `AGENTS.md` for project guidance.

## Build and run (two-terminal workflow)

Terminal 1 - run a profiled app (metrics server starts automatically on port 6770):

```bash
cargo run -p test-tokio-async --example long_running --features hotpath
```

Terminal 2 - launch the TUI console:

```bash
cargo run --bin hotpath --features tui -- console --metrics-port 6770
# optional: --refresh-interval <ms>
```

Self-contained demo with live sqlx/diesel/reqwest/axum traffic: `cargo run --bin hotpath --features tui,demo -- console`.

## Code map

Everything lives under `crates/hotpath/bin/hotpath/cmd/console/`:

- `app/keys.rs` - keyboard bindings
- `app/state.rs` + `app/data.rs` - app state and data model
- `views/` - one module per tab/subtab
- `http_worker.rs` - polls the metrics server endpoints
- `demo.rs` - the `demo` feature traffic generator

The TUI is a pure client of the metrics HTTP server - any data question ("what does the SQL subtab show?") resolves to the corresponding `Route` in `src/json.rs` plus the view module rendering it.

## Layout conventions

- Report tables that get too wide are split into stacked per-kind sub-tables sharing one selection cursor (e.g. rw_locks reads/writes, io reads/writes); the terminal report mirrors the same split.
- The Server subtab reuses the HTTP logs panel and its app state.
