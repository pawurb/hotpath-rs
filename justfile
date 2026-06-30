# Just configuration for hotpath-rs

# Default recipe
default:
    @just --list

# Run benchmark for current state
bench:
    bash scripts/bench.sh

# Run benchmark for current state (meta profiler)
bench_meta:
    bash scripts/bench_meta.sh

# Run benchmarks comparing two git refs
compare before after:
    bash scripts/compare.sh {{before}} {{after}}

# Run benchmarks comparing two git refs (meta profiler)
compare_meta before after:
    bash scripts/compare_meta.sh {{before}} {{after}}

# Run all tests
test_all:
    cargo run -p test-all-features --example all_noop
    cargo test --features hotpath --test guards -- --nocapture --test-threads=1
    cargo test --features hotpath --test functions_timing -- --nocapture --test-threads=1
    cargo test --features hotpath --test functions_alloc -- --nocapture --test-threads=1
    cargo test --features hotpath --test functions_cpu -- --nocapture --test-threads=1
    cargo test --features hotpath --test streams -- --nocapture --test-threads=1
    cargo test --features hotpath --test channels_crossbeam -- --nocapture --test-threads=1
    cargo test --features hotpath --test channels_crossbeam_wrap -- --nocapture --test-threads=1
    cargo test --features hotpath --test channels_std_wrap -- --nocapture --test-threads=1
    cargo test --features hotpath --test channels_tokio_wrap -- --nocapture --test-threads=1
    cargo test --features hotpath --test channels_ftc -- --nocapture --test-threads=1
    cargo test --features hotpath --test channels_asc -- --nocapture --test-threads=1
    cargo test --features hotpath --test channels_std -- --nocapture --test-threads=1
    cargo test --features hotpath --test channels_tokio -- --nocapture --test-threads=1
    cargo test --features hotpath --test channels_flume -- --nocapture --test-threads=1
    cargo test --features hotpath --test channels_flume_wrap -- --nocapture --test-threads=1
    cargo test --features hotpath --test rw_lock_std -- --nocapture --test-threads=1
    cargo test --features hotpath --test rw_lock_parking_lot -- --nocapture --test-threads=1
    cargo test --features hotpath --test mutex_std -- --nocapture --test-threads=1
    cargo test --features hotpath --test mutex_tokio -- --nocapture --test-threads=1
    cargo test --features hotpath --test mutex_async_lock -- --nocapture --test-threads=1
    cargo test --features hotpath --test threads -- --nocapture --test-threads=1
    cargo test --features hotpath --test tokio_runtime -- --nocapture --test-threads=1
    cargo test --features hotpath --test futures -- --nocapture --test-threads=1
    cargo test --features hotpath --test debug -- --nocapture --test-threads=1

# Serve the mdbook docs locally with live reload (http://localhost:3000).
# The production server + deploy live in the private hotpath-backend repo.
docs:
    cd docs && mdbook serve --open

# Fetch GitHub star badges locally for documentation
fetch-badges:
    #!/usr/bin/env bash
    set -euo pipefail
    DIR="docs/src/images"
    fetch() { sleep 2; echo "Fetching $2..."; curl -sL "https://img.shields.io/github/stars/${2}?style=social" -o "${DIR}/stars-${1}.svg"; }
    fetch easytier-easytier EasyTier/EasyTier
    fetch apache-opendal apache/opendal
    fetch marc2332-freya marc2332/freya
    fetch tqwewe-kameo tqwewe/kameo
    fetch tryandromeda-andromeda tryandromeda/andromeda
    fetch maplibre-martin maplibre/martin
    fetch pawurb-hotpath-rs pawurb/hotpath-rs

    echo "Badges saved to ${DIR}/"

cargo-publish:
    cargo publish -p hotpath-macros-meta
    cargo publish -p hotpath-meta
    cargo publish -p hotpath-macros
    cargo publish -p hotpath
