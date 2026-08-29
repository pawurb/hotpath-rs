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
    cargo test --features hotpath --test channels_asc_wrap -- --nocapture --test-threads=1
    cargo test --features hotpath --test channels_std -- --nocapture --test-threads=1
    cargo test --features hotpath --test channels_tokio -- --nocapture --test-threads=1
    cargo test --features hotpath --test channels_flume -- --nocapture --test-threads=1
    cargo test --features hotpath --test channels_flume_wrap -- --nocapture --test-threads=1
    cargo test --features hotpath --test rw_lock_std -- --nocapture --test-threads=1
    cargo test --features hotpath --test rw_lock_parking_lot -- --nocapture --test-threads=1
    cargo test --features hotpath --test mutex_std -- --nocapture --test-threads=1
    cargo test --features hotpath --test mutex_parking_lot -- --nocapture --test-threads=1
    cargo test --features hotpath --test mutex_tokio -- --nocapture --test-threads=1
    cargo test --features hotpath --test mutex_async_lock -- --nocapture --test-threads=1
    cargo test --features hotpath --test threads -- --nocapture --test-threads=1
    cargo test --features hotpath --test tokio_runtime -- --nocapture --test-threads=1
    cargo test --features hotpath --test futures -- --nocapture --test-threads=1
    cargo test --features hotpath --test io -- --nocapture --test-threads=1
    cargo test --features hotpath --test io_redis -- --nocapture --test-threads=1
    cargo test --features hotpath --test http_reqwest -- --nocapture --test-threads=1
    cargo test --features hotpath --test http_ureq -- --nocapture --test-threads=1
    cargo test --features hotpath --test server_axum -- --nocapture --test-threads=1
    cargo test --features hotpath --test sql_sqlite -- --nocapture --test-threads=1
    cargo test --features hotpath --test sql_pg -- --nocapture --test-threads=1
    cargo test --features hotpath --test diesel -- --nocapture --test-threads=1
    cargo test --features hotpath --test diesel_pg -- --nocapture --test-threads=1
    cargo test --features hotpath --test toasty_sqlite -- --nocapture --test-threads=1
    cargo test --features hotpath --test toasty_pg -- --nocapture --test-threads=1
    cargo test --features hotpath --test debug -- --nocapture --test-threads=1

# Run the TUI in demo mode with the Prometheus exporter on port 6772.
# Scrape it with `docker compose up -d prometheus grafana`:
# Grafana http://localhost:3009 (dashboard auto-provisioned), Prometheus http://localhost:9099.
# On native Linux add HOTPATH_PROMETHEUS_ADDR=0.0.0.0 so the Prometheus
# container can reach the exporter through the Docker bridge gateway.
demo:
    HOTPATH_PROMETHEUS=true cargo run --bin hotpath --features tui,hotpath,hotpath-alloc,demo -- console

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
    fetch rustfs-rustfs rustfs/rustfs
    fetch apache-opendal apache/opendal
    fetch maplibre-martin maplibre/martin
    fetch marc2332-freya marc2332/freya
    fetch parseablehq-parseable parseablehq/parseable
    fetch MapleTechLabs-maple MapleTechLabs/maple
    fetch pawurb-hotpath-rs pawurb/hotpath-rs

    echo "Badges saved to ${DIR}/"

cargo-publish:
    cargo publish -p hotpath-macros-meta
    cargo publish -p hotpath-meta
    cargo publish -p hotpath-macros
    cargo publish -p hotpath
