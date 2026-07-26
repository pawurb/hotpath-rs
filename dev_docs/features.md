# Features Reference

Where to find the authoritative definitions, plus behavior gotchas that the code can't show at a glance. See `CLAUDE.md` for project guidance.

## Sources of truth

- **Feature flags**: `[features]` in `crates/hotpath/Cargo.toml`. User-facing descriptions are spread across the per-subsystem pages in `docs/src/` (see `SUMMARY.md` for the index); `profiling_modes.md` covers only the profiling-mode flags (`hotpath`, `hotpath-alloc`, ...).
- **Attribute macros** (`#[main]`, `#[measure]`, `#[measure_all]`, `#[skip]`, `#[future_fn]`) and their parameters: `crates/hotpath-macros/src/lib.rs` (doc comments on each `#[proc_macro_attribute]`).
- **Declarative macros** (`measure_block!`, `dbg!`, `val!`, `gauge!`, `tokio_runtime!`): `crates/hotpath/src/lib_on.rs`. Wrapper macros (`channel!`, `stream!`, `future!`, `rw_lock!`, `mutex!`, `http!`, `io!`): `#[macro_export]` in the matching `crates/hotpath/src/lib_on/<subsystem>.rs`. Every macro has a no-op twin in `lib_off.rs`.
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

Macros:
- `#[measure]` `label = "..."`: duplicate labels panic at runtime. `impl_type = "Type"` is required for correct `hotpath-cpu` attribution on bare-annotated impl methods (`#[measure_all]` on inherent impls auto-injects it; trait impl methods never match, see `dev_docs/architecture.md`).
- `channel!`: `futures_channel::mpsc` is forwarder-only (`proxy = true`) and bounded variants need `capacity = N`. Bounded `std::sync::mpsc::sync_channel` also requires `capacity = N` matching the constructor argument (std exposes no capacity accessor; the wrapper rebuilds the channel) - omitting it panics at runtime.
- `http!`: times reqwest's `execute` future, which resolves when response headers arrive - body download is excluded. The `hotpath::wrap::reqwest::Client` alias resolves to `ClientWithMiddleware` with profiling on and the raw client with it off.
- `io!`: derefs to the wrapped value; `hotpath::io_unwrap(x)` peels the wrapper for consuming methods (identity with profiling off). `Seek`/`BufRead` delegation is not instrumented. `iter = true` gives per-instance rows - unbounded instance churn then grows profiler state.
- All guards are `#[must_use]`.

Env vars:
- `HOTPATH_KEEP_INLINE` is read at proc-macro expansion time - touch source or `cargo clean` after toggling.
- `HOTPATH_ALLOC_METRIC` panics on values other than `bytes`/`count`. `HOTPATH_ALLOC_CUMULATIVE` produces invalid results for recursive functions.
- `HOTPATH_SQL_RAW_LOGS` is off by default so bound literals (potentially sensitive) never reach the logs.
- `HOTPATH_TIME_SAMPLING_RATE=0.0` gives count-only mode (durations and io `Rate` show `-`). Per-resource `HOTPATH_<RESOURCE>_TIME_SAMPLING_RATE` variants take precedence.
- CPU report has no dedicated limit var - it uses `HOTPATH_FUNCTIONS_LIMIT` (fallback `HOTPATH_LIMIT`); wrapper `caller_name` is exempt.

## GitHub CI Integration

`hotpath-utils profile-pr` compares PR branch metrics against base and posts a diff comment. Two-workflow setup (`hotpath-profile` + `hotpath-comment`) for fork security; see `.github/workflows/` and `docs/src/github_ci.md`.
