#!/usr/bin/env bash
set -euo pipefail

DURATION="${BENCH_DURATION:-8s}"
CONCURRENCY="${BENCH_CONCURRENCY:-50}"

RAND_URL='http://127.0.0.1:3001(/|/profiling_modes|/functions|/data_flow|/mcp|/configuration|/sampling_comparison|/github_ci|/threads|/tokio_runtime|/robots.txt|/sitemap.xml)'

cd crates/hotpath-backend
cargo build --release --bin server --features=hotpath,hotpath-alloc

SERVER_LOG=$(mktemp)
RUST_LOG=none HOTPATH_SHUTDOWN_MS=10000 ../../target/release/server > "$SERVER_LOG" 2>&1 &
SERVER_PID=$!
trap "kill $SERVER_PID 2>/dev/null; wait $SERVER_PID 2>/dev/null; rm -f $SERVER_LOG" EXIT

for i in $(seq 1 50); do
    curl -sf http://127.0.0.1:3001/ > /dev/null && break
    sleep 0.1
done

echo "Starting load test $DURATION with $CONCURRENCY concurrent requests..."
oha -z "$DURATION" -c "$CONCURRENCY" --no-tui --rand-regex-url "$RAND_URL"

echo ""
echo "Waiting for hotpath report..."
sleep 4

echo ""
echo "==> Hotpath report:"
cat "$SERVER_LOG"

kill "$SERVER_PID" 2>/dev/null || true
wait "$SERVER_PID" 2>/dev/null || true
trap - EXIT
rm -f "$SERVER_LOG"
