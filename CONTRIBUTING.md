## Submitting Changes

1. Create a feature branch from `main`
2. Make your changes
3. Ensure all the [CI checks](#ci-checks) pass
4. Open a pull request against `main`
5. Always check `Allow edits and access to secrets by maintainers` so we can push fixes or rebases directly to your branch ([docs](https://docs.github.com/en/pull-requests/collaborating-with-pull-requests/working-with-forks/allowing-changes-to-a-pull-request-branch-created-from-a-fork))

## `meta` crates explained

Project maintains a complete copy of `hotpath` (`hotpath-meta`) and `hotpath-macros` (`hotpath-macros-meta`). All changes must be mirrored in their corresponding `-meta` crates. This adds some maintenance overhead, but it allows to benchmark the library using itself, which is an invaluable source of performance data and optimization insights.

A full copy is needed because a crate cannot depend on itself. Extracting shared core is also impractical, because `hotpath` uses a custom instrumentation logic (like `#[cfg_attr(feature = "hotpath-meta", hotpath_meta::measure_all)]` calls). If you have ideas for a cleaner way to implement self-profiling without full crate duplication, I'm open to suggestions.

To mirror changes from the source crates into the meta crates, you can use the [`syncmeta`](skills/syncmeta/SKILL.md) LLM skill. It applies diffs from `hotpath`/`hotpath-macros` to their meta counterparts while preserving meta-specific naming (feature flags, env vars, crate imports).

## Benchmarking `hotpath` 

### Self benchmarks

Install [just](https://github.com/casey/just) and run:

```bash
just bench_meta
```

Starts a hotpath TUI for 5 seconds, gathers performance metrics and prints the report on exit. 

To benchmark across git commits first build `hotpath-utils` CLI:

```bash
cargo build --bin hotpath-utils --features=utils
```

Now run:

```bash
just compare_meta main feature_branch
```

It benchmarks two versions of the library (branch names or commit SHAs are supported) and saves performance reports in `tmp/before.txt` and `tmp/after.txt`. If contributing any performance-related change please include both reports in the PR.

- `HOTPATH_TUI_TAB` - set values from 1 to 6, to open a different TUI tab and execute different codepaths in the benchmark (default `1`)
- `HOTPATH_BENCH_RELEASE` - set to `true` to run benchmarks with `--release` profile (default `false`)
- `HOTPATH_TUI_REFRESH_INTERVAL_MS` - configure data refresh interval, lower values will produce more data (default `10`)
- `HOTPATH_TUI_AUTO_EXPAND_LOGS` - Auto-open the logs panel once initial data arrives and pin selection to the given table index. Set to an integer (e.g. `0` for the first row, `2` for the third) (default: unset). 
- `HOTPATH_META_FOCUS` - filter which methods appear in the benchmark report by name. Plain text does substring matching; wrap in `/pattern/` for regex (e.g. `HOTPATH_META_FOCUS="/^(compute|process)/"`).

### Overhead benchmarks

Each `benchmark_*` example hammers an instrumented codepath in a tight loop. Run with `--features hotpath` to measure with instrumentation, and omit it to measure the uninstrumented baseline.

#### Timing

Prints total time and per-operation overhead on exit. The call count defaults to 100,000 and is configurable via `HOTPATH_BENCHMARK_NOOP_RUNS`.

```bash
cargo run --example benchmark_noop --features hotpath --release
```

#### Allocations

```bash
cargo run --example benchmark_alloc --features='hotpath,hotpath-alloc' --release
```

#### Mutexes and RwLocks

Each lock backend has a dedicated crate with a `benchmark_*` example that hammers a single instrumented lock in a tight, uncontended loop, so the measured time reflects per-acquisition instrumentation overhead. RwLock examples run a write loop followed by a read loop. The iteration count defaults to 1,000,000 and is configurable via `HOTPATH_LOCK_BENCH_RUNS`.

For each backend, run with `--features hotpath` to measure with instrumentation, and omit it to measure the uninstrumented baseline.

##### std Mutex

```bash
cargo run -p test-mutex-std --example benchmark_mutex_std --features hotpath --release
```

##### tokio Mutex

```bash
cargo run -p test-mutex-tokio --example benchmark_mutex_tokio --features hotpath --release
```

##### async-lock Mutex

```bash
cargo run -p test-mutex-async-lock --example benchmark_mutex_async_lock --features hotpath --release
```

##### std RwLock

```bash
cargo run -p test-rw-lock-std --example benchmark_rw_lock_std --features hotpath --release
```

##### tokio RwLock

```bash
cargo run -p test-rw-lock-tokio --example benchmark_rw_lock_tokio --features hotpath --release
```

##### parking_lot RwLock

```bash
cargo run -p test-rw-lock-parking-lot --example benchmark_rw_lock_parking_lot --features hotpath --release
```

##### async-lock RwLock

```bash
cargo run -p test-rw-lock-async-lock --example benchmark_rw_lock_async_lock --features hotpath --release
```

#### Channels

Each channel backend has a dedicated crate with a `benchmark_channel_*` example that hammers a single instrumented channel with send/recv cycles in a tight, uncontended loop, so the measured time reflects per-operation instrumentation overhead. The iteration count defaults to 1,000,000 and is configurable via `HOTPATH_CHANNEL_BENCH_RUNS`.

For each backend, run with `--features hotpath` to measure with instrumentation, and omit it to measure the uninstrumented baseline.

##### std channel

```bash
cargo run -p test-channels-std --example benchmark_channel_std --features hotpath --release
```

##### crossbeam channel

```bash
cargo run -p test-channels-crossbeam --example benchmark_channel_crossbeam --features hotpath --release
```

##### tokio channel

```bash
cargo run -p test-channels-tokio --example benchmark_channel_tokio --features hotpath --release
```

##### futures-channel

```bash
cargo run -p test-channels-ftc --example benchmark_channel_ftc --features hotpath --release
```

##### async-channel

```bash
cargo run -p test-channels-asc --example benchmark_channel_asc --features hotpath --release
```

##### flume channel

```bash
cargo run -p test-channels-flume --example benchmark_channel_flume --features hotpath --release
```

### Samply traces 

Analyze [Samply](https://github.com/mstange/samply) traces by running the instrumented benchmarks:

```bash
cargo install --locked samply
```

#### Timing

```bash
cargo build --example benchmark_noop --features hotpath --profile profiling && HOTPATH_BENCHMARK_NOOP_RUNS=5000000 samply record './target/profiling/examples/benchmark_noop'
```

#### Allocations

```bash
cargo build --example benchmark_alloc --features='hotpath,hotpath-alloc' --profile profiling && samply record './target/profiling/examples/benchmark_alloc'
```

## Running documentation server

Install the dependencies:

- https://github.com/rust-lang/mdBook
- https://github.com/pawurb/mdbook-reading-time 
- https://github.com/pawurb/mdbook-assets-hash 

```bash
cargo install mdbook
cargo install mdbook-reading-time
cargo install mdbook-assets-hash
```

`just server` - run the documentation server on `http://localhost:3001`

## CI checks

CI runs on `ubuntu-latest` against Rust `1.89`, `stable`, and `nightly`. You can run the same checks locally:

### Compilation checks

```bash
cargo check
cargo check --all-features
cargo check --features hotpath
cargo check --features "hotpath,hotpath-alloc"
cargo check --features "hotpath,hotpath-mcp"
cargo check --features "hotpath,hotpath-alloc-meta,hotpath-meta"
cargo check -p hotpath --bin hotpath --features=tui
cargo check --features='tui,hotpath,hotpath-meta,hotpath-alloc-meta,hotpath-mcp,hotpath-mcp-meta,dev' --bin hotpath
cargo check -p hotpath --bin hotpath-utils --features=utils
```

### Formatting and linting

```bash
cargo fmt --all --check
cargo clippy --all --features hotpath -- -D warnings
cargo clippy --all --all-features -- -D warnings
cargo clippy --all --features "hotpath,hotpath-alloc" -- -D warnings
```

### Tests

```bash
cargo test --lib --features hotpath
cargo test -p hotpath --bin hotpath --features=tui
cargo run -p test-all-features --example all_noop
cargo test --features hotpath --test guards -- --nocapture --test-threads=1
cargo test --features hotpath --test functions_timing -- --nocapture --test-threads=1
cargo test --features hotpath --test functions_alloc -- --nocapture --test-threads=1
cargo test --features hotpath --test functions_cpu -- --nocapture --test-threads=1
cargo test --features hotpath --test streams -- --nocapture --test-threads=1
cargo test --features hotpath --test channels_crossbeam -- --nocapture --test-threads=1
cargo test --features hotpath --test channels_ftc -- --nocapture --test-threads=1
cargo test --features hotpath --test channels_asc -- --nocapture --test-threads=1
cargo test --features hotpath --test channels_std -- --nocapture --test-threads=1
cargo test --features hotpath --test channels_tokio -- --nocapture --test-threads=1
cargo test --features hotpath --test channels_flume -- --nocapture --test-threads=1
cargo test --features hotpath --test rw_lock_std -- --nocapture --test-threads=1
cargo test --features hotpath --test rw_lock_parking_lot -- --nocapture --test-threads=1
cargo test --features hotpath --test mutex_std -- --nocapture --test-threads=1
cargo test --features hotpath --test mutex_tokio -- --nocapture --test-threads=1
cargo test --features hotpath --test mutex_async_lock -- --nocapture --test-threads=1
cargo test --features hotpath --test threads -- --nocapture --test-threads=1
cargo test --features hotpath --test tokio_runtime -- --nocapture --test-threads=1
cargo test --features hotpath --test futures -- --nocapture --test-threads=1
cargo test --features hotpath --test debug -- --nocapture --test-threads=1
```

Or run all integration tests at once:

```bash
just test_all
```

## Crates

| Crate | Description |
|-------|-------------|
| `hotpath` | Core library - profiling runtime, reporting, metrics server, MCP server, TUI binary |
| `hotpath-meta` | Mirror of the `hotpath` library, used to profile the profiler itself. |
| `hotpath-macros` | Procedural macros (`#[measure]`, `#[main]`, `#[future_fn]`, etc.) |
| `hotpath-macros-meta` | Mirror of the `hotpath-macros` library, used to profile the profiler itself. |
| `hotpath-backend` | Axum web server with mdbook for the `hotpath.rs` documentation site |
| `test-tokio-async` | Integration tests and examples using the Tokio runtime |
| `test-smol-async` | Integration tests and examples using the smol runtime |
| `test-all-features` | Tests with all feature flags enabled |
| `test-channels-tokio` | Tests for Tokio channels instrumentation |
| `test-channels-ftc` | Tests for futures channels instrumentation |
| `test-channels-crossbeam` | Tests for crossbeam channels instrumentation |
| `test-channels-std` | Tests for std channels instrumentation |
| `test-streams` | Tests for streams instrumentation |
| `test-futures` | Tests for futures instrumentation |
| `test-debug` | Tests for debug metrics functionality |

## License

By contributing, you agree that your contributions will be licensed under the MIT License.
