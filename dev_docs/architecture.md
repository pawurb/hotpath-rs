# Architecture Reference

Runtime internals: background workers, servers, and CPU sampling. See `CLAUDE.md` for project guidance and the high-level pipeline description.

## Background workers

Each subsystem spawns a dedicated worker thread named `hp-<subsystem>` from its `crates/hotpath/src/lib_on/<subsystem>.rs` (grep `"hp-` for the full list). Exception: `hp-functions` is spawned from `lib_on/hotpath_guard.rs`, where its worker loop also lives. Workers sweep the per-thread SPSC queues on the drain interval, maintain running statistics (HDR histograms for percentiles), drain once more at shutdown, and hand aggregated state to report generation. `hp-threads` and `hp-runtime` are samplers instead of queue consumers (configurable intervals).

## HTTP Metrics Server

tiny_http server, localhost-only, port 6770 by default (`HOTPATH_METRICS_PORT`; disable with `HOTPATH_METRICS_SERVER_OFF`). Optional shared-secret auth: `HOTPATH_METRICS_AUTH_TOKEN` (read once into a `LazyLock`) must be sent verbatim in the `Authorization` header on every route, else `401`; the constant-time comparison lives in `crates/hotpath/src/auth.rs` and is shared with the MCP server. The TUI sends it from the same env var or `--metrics-auth-token`. Feeds the TUI. Implementation: `crates/hotpath/src/metrics_server.rs`; the route table is the `Route` enum in `crates/hotpath/src/json.rs` - read that enum for the current endpoint list (per-section metrics as JSON plus `/{id}/logs` sub-routes and the CPU snapshot trigger).

## MCP Server

`crates/hotpath/src/mcp_server.rs`, behind the `hotpath-mcp` feature. Port 6771 (`HOTPATH_MCP_PORT`), endpoint `POST /mcp` (Streamable HTTP). The tool list is the set of `#[tool(...)]` methods in that file - roughly one tool per metrics endpoint plus log variants. Auth: `HOTPATH_MCP_AUTH_TOKEN` sets the expected value; clients must send it verbatim in the `Authorization` header (no `Bearer` prefix handling).

## Tokio Runtime Monitoring

`hotpath::tokio_runtime!()` (behind `tokio` feature) spawns `hp-runtime`, which polls `tokio::runtime::RuntimeMetrics` into a static snapshot - see `crates/hotpath/src/lib_on/tokio_runtime.rs` for the collected fields. Some metrics (poll counts, steal ops, blocking threads, IO driver) require `RUSTFLAGS="--cfg tokio_unstable"`. Exposed via `GET /tokio_runtime` and the TUI runtime tab.

## CPU Sampling (`hotpath-cpu` feature, macOS and Linux)

CPU samples are attributed to instrumented functions via an external `samply` worker. Code map:

- `crates/hotpath/bin/hotpath-samply/main.rs` - wrapper binary spawned as a child of the host process; runs `samply record --pid <host>` and writes a gzipped profile under `/tmp/hotpath/<session_id>/hp.json.gz`.
- `crates/hotpath/src/lib_on/functions/cpu/autospawn.rs` - manages the wrapper child lifecycle (`start()` from `HotpathGuard::new()`, `stop()` via sentinel files, re-invoked after each on-demand snapshot).
- `crates/hotpath/src/lib_on/functions/cpu/samply.rs` - parses the profile and resolves samples to instrumented function names; on guard drop `build_cpu_report_from_path()` produces the `CpuReport`.

Gotchas that aren't obvious from the code structure:

- The on-disk profile is samply's native Firefox Profiler "processed profile" JSON (gzipped), NOT Google pprof.
- Symbol resolution parses the on-disk binary via `object`, demangles with `rustc-demangle`, and strips `::h<hash>` suffixes. Only the primary library (matching `current_exe` basename) is indexed.
- Symbols match longest-prefix, so `::{{closure}}` and hash suffixes attribute back to the parent function. Trait impl symbols (`<Type as Trait>::method`) are NOT matched - only inherent impl/free function names. Hence the `impl_type` macro requirement: bare `#[measure]` on an impl method needs `impl_type = "TypeName"` so the registered name `module::Type::method` matches the demangled symbol (`#[measure_all]` on inherent impls auto-injects it).
- Per-sample weight comes from `threadCPUDelta` (µs) when present, falling back to `weight` or 1.
- Attribution is exclusive by default (deepest matching frame); `HOTPATH_CPU_INCLUSIVE=1` credits every matching frame, deduping recursive frames per sample.
- On-demand snapshots: `POST /functions_cpu/snapshot` runs a background `hp-cpu-snapshot` thread; `GET /functions_cpu` reports status (`idle|capturing|ready|error`). TUI CPU subtab: `c` captures, `f` opens `samply load`.
