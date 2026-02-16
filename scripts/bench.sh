#!/usr/bin/env bash
set -euo pipefail

BEFORE_REF="$1"
AFTER_REF="$2"
ORIGINAL_REF=$(git rev-parse --abbrev-ref HEAD)
# If detached HEAD, use commit hash
if [ "$ORIGINAL_REF" = "HEAD" ]; then
    ORIGINAL_REF=$(git rev-parse HEAD)
fi

BENCH_CMD="cargo run --features='tui,hotpath,hotpath-meta,hotpath-alloc-meta' --bin hotpath"

run_bench() {
    local ref="$1"
    local output="$2"
    echo "==> Checking out $ref"
    git checkout "$ref"
    echo "==> Running benchmark on $ref..."
    HOTPATH_TUI_TAB=1 \
    HOTPATH_META_REPORT='functions-timing,functions-alloc,threads' \
    HOTPATH_META_OUTPUT_PATH="$output" \
    HOTPATH_META_SHUTDOWN_MS=10000 \
    HOTPATH_META_EXCLUDE_WRAPPER=true \
    RUSTFLAGS="--cfg tokio_unstable" \
    $BENCH_CMD
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

run_bench "$BEFORE_REF" "tmp/before.txt"
run_bench "$AFTER_REF" "tmp/after.txt"

echo ""
echo "Done. Compare results:"
echo "  tmp/before.txt ($BEFORE_REF)"
echo "  tmp/after.txt  ($AFTER_REF)"
