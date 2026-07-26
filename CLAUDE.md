# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

hotpath-rs is a lightweight, feature-gated Rust profiler that tracks function execution time, memory allocations, channels, streams, futures, locks, SQL queries, HTTP requests, byte-level I/O, and threads. All instrumentation is behind the `hotpath` Cargo feature: with it off, every macro is a no-op and no dependencies are compiled.

Workspace layout:
- `crates/hotpath` - Main library with profiling runtime, reporting, metrics/MCP servers, and the TUI/CLI binaries
- `crates/hotpath-macros` - Procedural macros (`#[measure]`, `#[main]`, `#[future_fn]`, ...)
- `crates/test-*` - One integration-test crate per instrumented subsystem or third-party integration; the current list is the `members` array in the root `Cargo.toml`
  - `test-toasty` is NOT a workspace member: toasty's rusqlite and the workspace's sqlx-sqlite have conflicting `links = "sqlite3"` values, so it's built via `cargo run --manifest-path crates/test-toasty/Cargo.toml ...`
- `crates/hotpath-meta` / `crates/hotpath-macros-meta` - Copies of hotpath used to profile the profiler itself (not intended for external use)
- `docs/` - mdBook source for the hotpath.rs documentation site (the Axum web server that builds/serves it lives in a separate private repo at `../hotpath-backend`)

## Reference Docs

The dev_docs point to where things are defined in source and record gotchas the code can't show - read them only when a task needs the specifics:

- `dev_docs/features.md` - Where feature flags, macros, the builder API, and env vars are defined, plus behavior gotchas
- `dev_docs/architecture.md` - Background workers, metrics/MCP server code map, CPU sampling internals and pitfalls
- `dev_docs/tui.md` - TUI build/usage, console code map, layout conventions
- `dev_docs/testing.md` - Integration-test patterns (`crates/hotpath/tests/`): polling the metrics endpoint vs parsing the guard-drop JSON report, example code, and test-file conventions. Read before writing or modifying an integration test.
- `CONTRIBUTING.md` - Meta-crate mirroring (syncmeta skill), self and overhead benchmark commands (`just bench_meta`, `just compare_meta`, per-subsystem `benchmark_*` examples), samply tracing, the exact CI check commands to run locally, and docs build prerequisites

Common sources of truth: the metrics API is the `Route` enum in `crates/hotpath/src/json.rs` + `metrics_server.rs`, MCP tools are the `#[tool]` methods in `mcp_server.rs`, env vars are parsed mainly in `lib_on/hotpath_guard.rs` (user-facing reference: `docs/src/configuration.md`), feature flags are in `crates/hotpath/Cargo.toml`, macro parameters are doc-commented in `crates/hotpath-macros/src/lib.rs`.

## Development Commands

```bash
cargo build                                # build
cargo build --features hotpath             # build with profiling enabled
cargo check --bin hotpath --features tui   # check the TUI binary compiles

# Run an example from a test crate (each example lists its exact run command in its top comment)
cargo run -p test-tokio-async --example basic --features hotpath
cargo run -p test-channels-tokio --example basic_tokio --features hotpath

# Profiling modes are combined via features, e.g.
cargo run -p test-tokio-async --features='hotpath,hotpath-alloc' --example basic
```

Just recipes:
```bash
just test_all      # Run all integration tests
just docs          # Serve the mdbook docs locally with live reload (http://localhost:3000)
```

TUI quickstart (details in `dev_docs/tui.md`): run a profiled example in one terminal (metrics server starts on port 6770 by default), then `cargo run --bin hotpath --features tui -- console --metrics-port 6770` in another.

## Architecture

**Profiling pipeline**: Measurements flow from instrumented code -> per-thread lock-free chunked SPSC queue (`lib_on/batch.rs`) -> background worker thread (single consumer, sweeps all queues every 50ms and once more at shutdown) -> statistics aggregation -> report generation on program exit. The producer hot path is a plain slot store plus one `Release` publish of the chunk length - no mutex, no RMW atomic - and queues remain drainable from the worker at any moment, so events buffered on parked threads (e.g. idle tokio workers) still reach the final report. Producers are gated by a per-registry `active` flag so events cannot accumulate unbounded when no worker is consuming.

Each subsystem has a dedicated background worker thread named `hp-<subsystem>`, spawned from its `lib_on/<subsystem>.rs` - see `dev_docs/architecture.md`.

**Feature gating**: `lib.rs` orchestrates via `cfg_if!`; `lib_on.rs` is the enabled implementation, `lib_off.rs` the no-op stubs. Every public macro must exist in both. Time profiling uses `time::TimeGuard`; allocation profiling uses a custom global allocator with `alloc::MeasurementGuard`.

**Async caveats**: async function profiling works on any async runtime with no runtime-specific feature flag (see `crates/test-smol-async`); the `tokio` feature is only needed for tokio-specific integrations (tokio channels/locks, async `io!` traits, `tokio_runtime!()`). Allocation profiling works for async functions on any async runtime: allocations are measured around each instrumented `poll()` (`measure_poll_alloc` in `lib_on/futures/wrapper.rs`) and aggregated across threads via `AsyncAllocBridge`, which the async measurement guard snapshots on drop. 

**Servers**: a localhost-only metrics HTTP server (tiny_http, port 6770) starts by default and feeds the TUI; the `Route` enum in `src/json.rs` is the route table - read it plus `metrics_server.rs` when modifying the API. An optional MCP server (`hotpath-mcp` feature, port 6771) lives in `mcp_server.rs`.

**CPU sampling** (`hotpath-cpu`, macOS/Linux): an external `samply` worker records the host process; symbols are resolved from the binary and matched back to instrumented function names. Pitfall: bare `#[hotpath::measure]` on a method inside an `impl` block needs `impl_type = "TypeName"` for CPU attribution to match the demangled symbol (`#[measure_all]` on inherent impls auto-injects it; trait impl methods never match). Full internals in `dev_docs/architecture.md`.

**Source tracking**: `lib_on/caller_stack.rs` maintains a per-thread stack of instrumented function names; SQL queries and HTTP requests record the innermost instrumented caller as their `source`.

### Key Files

- `crates/hotpath/src/lib.rs` / `lib_on.rs` / `lib_off.rs` - Entry points (feature orchestration, enabled impl, no-op stubs)
- `crates/hotpath-macros/src/lib.rs` - Procedural macro implementations
- `crates/hotpath/src/lib_on/<subsystem>.rs` (+ same-named subdir) - One module per instrumented subsystem: `functions` (timing/alloc, + `functions/cpu/` for sampling), `channels`, `streams`, `futures`, `rw_locks`, `mutexes`, `sql`, `http`, `io`, `threads`, `tokio_runtime`, `debug`
- `crates/hotpath/src/metrics_server.rs` + `src/json.rs` - Metrics HTTP server and its `Route` table / JSON types
- `crates/hotpath/src/mcp_server.rs` - MCP server
- `crates/hotpath/bin/hotpath/` - TUI binary (`cmd/console/` holds app state, views, HTTP client)
- `crates/hotpath/bin/hotpath-utils/` - CLI for A/B benchmarks (`compare`) and CI PR comments (`profile-pr`)
- `crates/hotpath/bin/hotpath-samply/` - CPU sampling wrapper binary

### Documentation

The mdBook source lives in `docs/` (book.toml, src/, theme/). Serve a live-reloading local preview with `just docs` (`mdbook serve` on `http://localhost:3000`). Requires `mdbook`, `mdbook-assets-hash`, and `mdbook-reading-time` on PATH. The Axum web server that builds and serves the production site lives in a separate **private** repo at `../hotpath-backend`, which consumes `docs/` via a `html_src` symlink.

## Code style

Never use `super` for imports, only `crate` and absolute paths.

Default to `pub(crate)` for new functions, structs, and fields. Only use `pub` when the item is part of the public API or re-exported from `lib.rs`.

Never use em dashes. Always use a regular hyphen (-) instead. This applies everywhere, especially in code comments.

NEVER use `mod.rs` files, so instead of `functions/mod.rs` use `functions.rs`.

Every example in a test crate starts with a `//! Run with:` header comment containing its exact cargo command.

## Other

Never read hotpath-meta and hotpath-macros-meta crates when exploring and planning. Changes to `crates/hotpath` / `crates/hotpath-macros` must eventually be mirrored into their `-meta` counterparts (see CONTRIBUTING.md; the `syncmeta` skill applies the diffs), but perform that mirroring only when explicitly asked - it is triggered as a separate step, not as part of every change.

NEVER instrument hotpath-meta crates using hotpath_meta, it won't work.

NEVER run "just test_all" unless explicitly asked, it's slow.
