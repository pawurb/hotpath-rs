## Submitting Changes

1. Create a feature branch from `main`
2. Make your changes
3. Ensure all the [CI checks](#ci-checks) pass
4. Open a pull request against `main`

## `meta` crates explained

Project maintains a complete copy of `hotpath` (`hotpath-meta`) and `hotpath-macros` (`hotpath-macros-meta`). All changes must be mirrored in their corresponding `-meta` crates. This adds some maintenance overhead, but it allows to benchmark the library using itself, which is an invaluable source of performance data and optimization insights.

A full copy is needed because a crate cannot depend on itself. Extracting shared core is also impractical, because `hotpath` uses a custom instrumentation logic (like `#[cfg_attr(feature = "hotpath-meta", hotpath_meta::measure_all)]` calls). If you have ideas for a cleaner way to implement self-profiling without full crate duplication, I'm open to suggestions.

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
- `HOTPATH_META_FOCUS` - filter which methods appear in the benchmark report by name. Plain text does substring matching; wrap in `/pattern/` for regex (e.g. `HOTPATH_META_FOCUS="/^(compute|process)/"`).

### Hyperfine benchmarks

Benchmark `hotpath` overhead of profiling 100k method calls with [hyperfine](https://github.com/sharkdp/hyperfine):

Timing:
```bash
# With instrumentation
cargo build --example benchmark_noop --features hotpath --release
hyperfine --warmup 3 './target/release/examples/benchmark_noop'

# Without instrumentation
cargo build --example benchmark_noop --release
hyperfine --warmup 3 './target/release/examples/benchmark_noop'
```

Allocations:

```bash
# With instrumentation
cargo build --example benchmark_alloc --features='hotpath,hotpath-alloc' --release
hyperfine --warmup 3 './target/release/examples/benchmark_alloc'

# Without instrumentation
cargo build --example benchmark_alloc --release
hyperfine --warmup 3 './target/release/examples/benchmark_alloc'
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
cargo test --features hotpath --test functions -- --nocapture --test-threads=1
cargo test --example unit_test --features hotpath -- --nocapture --test-threads=1
cargo run --example basic_std
cargo test --features hotpath --test streams -- --nocapture --test-threads=1
cargo test --features hotpath --test channels_crossbeam -- --nocapture --test-threads=1
cargo test --features hotpath --test channels_ftc -- --nocapture --test-threads=1
cargo test --features hotpath --test channels_asc -- --nocapture --test-threads=1
cargo test --features hotpath --test channels_std -- --nocapture --test-threads=1
cargo test --features hotpath --test channels_tokio -- --nocapture --test-threads=1
cargo test --features hotpath --test threads -- --nocapture --test-threads=1
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
