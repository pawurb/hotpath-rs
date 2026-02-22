#!/usr/bin/env bash
set -euo pipefail

BEFORE_REF="$1"
AFTER_REF="$2"
ORIGINAL_REF=$(git rev-parse --abbrev-ref HEAD)
# If detached HEAD, use commit hash
if [ "$ORIGINAL_REF" = "HEAD" ]; then
    ORIGINAL_REF=$(git rev-parse HEAD)
fi

RELEASE_FLAG=""
if [ "${HOTPATH_BENCH_RELEASE:-}" = "true" ]; then
    RELEASE_FLAG="--release"
fi
BENCH_CMD="cargo run $RELEASE_FLAG -p hotpath --features=tui,hotpath,hotpath-meta,hotpath-alloc-meta --bin hotpath"

run_bench() {
    local ref="$1"
    local output="$2"
    local resolved
    resolved=$(git rev-parse --short "$ref")
    local label="$ref"
    [ "$ref" != "$resolved" ] && label="$ref ($resolved)"
    echo "==> Checking out $ref"
    git checkout "$ref"
    local -a bench_env=(
        HOTPATH_TUI_TAB=${HOTPATH_TUI_TAB:-1}
        HOTPATH_TUI_REFRESH_INTERVAL_MS=${HOTPATH_TUI_REFRESH_INTERVAL_MS:-10}
        HOTPATH_META_REPORT='functions-timing,functions-alloc,threads'
        HOTPATH_META_OUTPUT_FORMAT=json
        HOTPATH_META_OUTPUT_PATH="$output"
        HOTPATH_META_SHUTDOWN_MS=5000
        HOTPATH_META_TIMEOUT_MS=5000
        HOTPATH_META_EXCLUDE_WRAPPER=true
        HOTPATH_META_REPORT_LABEL="$label"
        RUSTFLAGS='--cfg tokio_unstable'
    )
    echo "==> Running: ${bench_env[*]} $BENCH_CMD"
    env "${bench_env[@]}" $BENCH_CMD
    echo "==> Results saved to $output"
}

if ! git diff --quiet || ! git diff --cached --quiet; then
    echo "Error: uncommitted changes. Commit or stash before running." >&2
    exit 1
fi

mkdir -p tmp

cleanup() {
    echo "==> Restoring to $ORIGINAL_REF"
    git checkout "$ORIGINAL_REF"
}
trap cleanup EXIT

run_bench "$BEFORE_REF" "tmp/before.json"
run_bench "$AFTER_REF" "tmp/after.json"

reset

cargo run $RELEASE_FLAG -p hotpath --features=utils --bin hotpath-utils -- \
    compare --before-json-path tmp/before.json --after-json-path tmp/after.json 
