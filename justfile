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
    cargo test --features hotpath --test channels_ftc -- --nocapture --test-threads=1
    cargo test --features hotpath --test channels_asc -- --nocapture --test-threads=1
    cargo test --features hotpath --test channels_std -- --nocapture --test-threads=1
    cargo test --features hotpath --test channels_tokio -- --nocapture --test-threads=1
    cargo test --features hotpath --test channels_flume -- --nocapture --test-threads=1
    cargo test --features hotpath --test rw_lock_std -- --nocapture --test-threads=1
    cargo test --features hotpath --test rw_lock_parking_lot -- --nocapture --test-threads=1
    cargo test --features hotpath --test mutex_std -- --nocapture --test-threads=1
    cargo test --features hotpath --test mutex_tokio -- --nocapture --test-threads=1
    cargo test --features hotpath --test threads -- --nocapture --test-threads=1
    cargo test --features hotpath --test tokio_runtime -- --nocapture --test-threads=1
    cargo test --features hotpath --test futures -- --nocapture --test-threads=1
    cargo test --features hotpath --test debug -- --nocapture --test-threads=1

# Start the dev server
server: docs
    cd crates/hotpath-backend && cargo run --bin server

# Build mdbook docs and clean .html links
docs:
    cd crates/hotpath-backend/html_src && mdbook build
    cargo run -p hotpath-backend --bin clean-html-links crates/hotpath-backend/html

# Deploy to remote server
deploy: docs
    cd crates/hotpath-backend && ./deploy.sh

# Deploy, restart server, and purge cache
release: deploy
    cd crates/hotpath-backend && ./remote/restart.sh
    just clean-cache
    echo "Release deployed and server restarted"

# Fetch GitHub star badges locally for documentation
fetch-badges:
    #!/usr/bin/env bash
    set -euo pipefail
    DIR="crates/hotpath-backend/html_src/src/images"
    fetch() { sleep 2; echo "Fetching $2..."; curl -sL "https://img.shields.io/github/stars/${2}?style=social" -o "${DIR}/stars-${1}.svg"; }
    fetch apache-opendal apache/opendal
    fetch apache-horaedb apache/horaedb
    fetch marc2332-freya marc2332/freya
    fetch tqwewe-kameo tqwewe/kameo
    fetch tryandromeda-andromeda tryandromeda/andromeda
    fetch maplibre-martin maplibre/martin
    fetch pawurb-hotpath-rs pawurb/hotpath-rs

    echo "Badges saved to ${DIR}/"

# Benchmark the documentation server
bench_docs: docs
    bash scripts/bench_docs.sh

# Purge Cloudflare cache
clean-cache:
    #!/usr/bin/env bash
    source crates/hotpath-backend/.envrc
    curl -s -X POST "https://api.cloudflare.com/client/v4/zones/${CLOUDFLARE_ZONE_ID}/purge_cache" \
        -H "X-Auth-Email: ${CLOUDFLARE_EMAIL}" \
        -H "X-Auth-Key: ${CLOUDFLARE_API_KEY}" \
        -H "Content-Type: application/json" \
        --data '{"purge_everything":true}'

cargo-publish:
    cargo publish -p hotpath-macros-meta
    cargo publish -p hotpath-meta
    cargo publish -p hotpath-macros
    cargo publish -p hotpath
