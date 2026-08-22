# Live TUI Console Reference

Terminal-based TUI for real-time monitoring. See `CLAUDE.md` for project guidance.

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

- `app/keys.rs` - keyboard bindings (authoritative list; basics: `q` quit, `p` pause, `o` logs panel, `j`/`k` navigate, number keys switch top-level tabs)
- `app/state.rs` + `app/data.rs` - app state and data model
- `views/` - one module per tab/subtab (`functions_timing`, `functions_memory`, `functions_cpu`, `data_flow/` for channels/streams/futures/locks, `io/` for SQL/HTTP/Server/bytes, `threads`, `debug`, `runtime`); what each tab shows is defined by its view module and the JSON types in `crates/hotpath/src/json.rs`
- `http_worker.rs` - polls the metrics server endpoints
- `demo.rs` - the `demo` feature traffic generator

The TUI is a pure client of the metrics HTTP server - any data question ("what does the SQL subtab show?") resolves to the corresponding `Route` in `src/json.rs` plus the view module rendering it.

## Layout conventions

- Report tables that get too wide are split into stacked per-kind sub-tables sharing one selection cursor (e.g. rw_locks reads/writes, io reads/writes); the terminal report mirrors the same split.
- Locks and io bytes are table-only (no per-event logs panel); channels, streams, futures, functions, SQL, HTTP, and Server have logs panels (`log = true` where applicable). SQL/HTTP log panels show the `source` attribution from `caller_stack.rs`. The Server subtab reuses the HTTP logs panel and its `http_logs`/`show_http_logs` app state (same `JsonHttpLogsList` shape; the pane resets on every subtab switch).
