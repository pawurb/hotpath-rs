# Features Reference

Where to find the authoritative definitions, plus behavior gotchas that the code can't show at a glance. See `CLAUDE.md` for project guidance.

## Sources of truth

- **Feature flags**: `[features]` in `crates/hotpath/Cargo.toml`. User-facing descriptions are spread across the per-subsystem pages in `docs/src/` (see `SUMMARY.md` for the index); `profiling_modes.md` covers only the profiling-mode flags (`hotpath`, `hotpath-alloc`, ...).
- **Attribute macros** (`#[main]`, `#[measure]`, `#[measure_all]`, `#[skip]`, `#[future_fn]`) and their parameters: `crates/hotpath-macros/src/lib.rs` (doc comments on each `#[proc_macro_attribute]`).
- **Declarative macros** (`measure_block!`, `dbg!`, `val!`, `gauge!`, `tokio_runtime!`): `crates/hotpath/src/lib_on.rs`. Wrapper macros (`channel!`, `stream!`, `future!`, `rw_lock!`, `mutex!`, `http!`, `axum!`, `io!`): `#[macro_export]` in the matching `crates/hotpath/src/lib_on/<subsystem>.rs`. Every macro has a no-op twin in `lib_off.rs`.
- **Builder API**: `HotpathGuardBuilder` in `crates/hotpath/src/lib_on/hotpath_guard.rs` (defaults live in its field initializers).
- **Environment variables**: user-facing reference in `docs/src/configuration.md`; discover parse sites with `grep -rn 'HOTPATH_' crates/hotpath/src crates/hotpath/bin` (main site: `lib_on/hotpath_guard.rs`; TUI vars in `bin/hotpath/cmd/console*`; CPU vars in `lib_on/functions/cpu/`).
- **Usage examples**: every `crates/test-*` example's top comment contains its exact cargo command (usually a `//! Run with:` header).

## Running examples

```bash
cargo run -p test-tokio-async --features=hotpath --example basic
cargo run -p test-tokio-async --features='hotpath,hotpath-alloc' --example basic   # + alloc profiling
cargo run -p test-smol-async --features=hotpath --example basic_smol               # non-tokio runtime
```

## Gotchas (not obvious from signatures)

Features:
- `tokio` is only for tokio-specific integrations (tokio channels/locks, async `io!` traits, `tokio_runtime!()`). Async function profiling itself is runtime-agnostic and needs no feature.
- `sqlx`/`toasty` layers pull in no sqlx/toasty dependency - they are generic `tracing` layers reading event fields (`sqlx::query` / `toasty::query` targets; don't globally filter those out). One sqlx layer covers 0.8 and 0.9. Toasty key-value (DynamoDB) ops carry no SQL and are skipped.
- `demo` implies `hotpath` and pulls sqlx/diesel/reqwest clients only for the TUI demo, never in plain `tui` or library builds.
- `hotpath-cloud` (`lib_on/cloud.rs`, not user-documented yet) does not imply `hotpath`; with `hotpath` on it uploads the `JsonReport` from `HotpathGuard::drop` to hotpath.rs when `HOTPATH_UPLOAD=1`, via a sync `ureq` client (the guard may outlive the tokio runtime). Needs `ACTIONS_ID_TOKEN_REQUEST_URL`/`_TOKEN` (GitHub Actions with `id-token: write`), otherwise prints a skip line. `HOTPATH_BENCHMARK` names the series (default `default`). The base URL is hard-coded to `https://hotpath.rs`. Upload is additive to the configured output format and never affects the exit code.

Macros:
- `#[measure]` `label = "..."`: duplicate labels panic at runtime. `impl_type = "Type"` is required for correct `hotpath-cpu` attribution on bare-annotated impl methods (`#[measure_all]` on inherent impls auto-injects it; trait impl methods never match, see `dev_docs/architecture.md`).
- `channel!`: `futures_channel::mpsc` is forwarder-only (`proxy = true`) and bounded variants need `capacity = N`. Bounded `std::sync::mpsc::sync_channel` also requires `capacity = N` matching the constructor argument (std exposes no capacity accessor; the wrapper rebuilds the channel) - omitting it panics at runtime.
- `channel!`/`stream!` default to one entry per `(call site, message/item type)` - repeat registrations reuse the cached id from `CHANNEL_SOURCE_IDS` (`channels.rs`) / `STREAM_SOURCE_IDS` (`streams.rs`) and only bump the entry's `Inst` count, so state stays bounded by call-site count. `iter = true` gives per-instance rows (suffixed `label-2`, `label-3`, ...) - unbounded instance churn then grows profiler state, same tradeoff as `io!`. The first registration's `label` and `channel_type` win for the aggregated entry; a variable-capacity expression at one call site is recorded under the first capacity seen.
- `http!`: times reqwest's `execute` future, which resolves when response headers arrive - body download is excluded. The `hotpath::wrap::reqwest::Client` alias resolves to `ClientWithMiddleware` with profiling on and the raw client with it off. For ureq (`ureq-3` feature) the macro takes a `ConfigBuilder<AgentScope>` and returns it with `UreqHttpMiddleware` appended (ureq only accepts middleware at config-build time; no type alias needed since the type is unchanged). ureq's `Error::StatusCode` is mapped back to a status so 4xx/5xx aren't miscounted as transport errors; an explicit scheme-default port (`:443`/`:80`) is dropped from the endpoint key because `Uri::port_u16` keeps it. Any new HTTP/SQL front-end feature must be added to the `cfg_if!` gates in `lib_on/caller_stack.rs` and `lib_on/functions.rs` (`await_with_caller_scope`) or the `Source` column stays empty.
- `axum!` (`axum-0-8`): expands to `router.layer(AxumLayer::new())`, so it must wrap the finished router - `Router::layer` ignores routes added afterwards. `AxumLayer` is a hand-written tower `Layer`/`Service` (`lib_on/server/axum_08.rs`, generic over request/response bodies), not `from_fn`, so no per-request boxing. Route key is `METHOD MatchedPath`; `MatchedPath` is absent for fallbacks and `nest_service` targets, in which case the raw path goes through `http::normalize::normalize_endpoint`. Timing stops when the response head is produced (body streaming excluded, symmetric with `http!`). Server logs reuse `HttpLogEntry`/`JsonHttpLogsList`; the TUI Server subtab reuses the HTTP logs pane state. Route scoping: `AxumResponseFuture::poll` enters `caller_stack::enter_route` (a save/restore thread-local `Cell`, separate from the caller stack) around the inner poll, and SQL/HTTP front-ends read `current_route()` next to `current_caller()`; `route` is the first element of the `SqlKey`/`HttpKey` tuple and the `Route` column renders only when some entry has one. Only matched templates are interned to `&'static str` (`intern_route`, capped at `HOTPATH_ENTRIES_LIMIT`; templates past the cap get no route context) - unmatched raw paths never enter the interner. `stop_server_events` turns scoping off so post-shutdown requests stop interning. `HOTPATH_ROUTE_SCOPE=0` / `route_scope(false)` is resolved in `HotpathGuardBuilder::build`, so a layer created before the guard still honors it at request time.
- `io!`: derefs to the wrapped value; `hotpath::io_unwrap(x)` peels the wrapper for consuming methods (identity with profiling off). `Seek`/`BufRead` delegation is not instrumented. `iter = true` gives per-instance rows - unbounded instance churn then grows profiler state.
- All guards are `#[must_use]`.

Env vars:
- `HOTPATH_KEEP_INLINE` is read at proc-macro expansion time - touch source or `cargo clean` after toggling.
- `HOTPATH_ALLOC_METRIC` panics on values other than `bytes`/`count`. `HOTPATH_ALLOC_CUMULATIVE` produces invalid results for recursive functions.
- `HOTPATH_SQL_RAW_LOGS` is off by default so bound literals (potentially sensitive) never reach the logs.
- `HOTPATH_TIME_SAMPLING_RATE=0.0` gives count-only mode (durations and io `Rate` show `-`). Per-resource `HOTPATH_<RESOURCE>_TIME_SAMPLING_RATE` variants take precedence.
- CPU report has no dedicated limit var - it uses `HOTPATH_FUNCTIONS_LIMIT` (fallback `HOTPATH_LIMIT`); wrapper `caller_name` is exempt.

## GitHub CI Integration

`hotpath-utils profile-pr` compares PR branch metrics against base and posts a diff comment. Two-workflow setup (`hotpath-profile` + `hotpath-comment`) for fork security; see `docs/src/github_ci.md`.
