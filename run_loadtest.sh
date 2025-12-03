#!/bin/bash

METRICS_HOST="${METRICS_HOST:-127.0.0.1}"
METRICS_PORT="${METRICS_PORT:-6770}"
NUM_REQUESTS="${NUM_REQUESTS:-1000}"
CONCURRENCY_LIMIT="${CONCURRENCY_LIMIT:-1}"
ENDPOINT="${ENDPOINT:-/metrics}"

# Construct the target URL
LOAD_TEST_URL="http://${METRICS_HOST}:${METRICS_PORT}${ENDPOINT}"

echo "HotPath Load Test (using oha)"
echo "=============================="
echo ""
echo "Configuration:"
echo "  Target URL:      ${LOAD_TEST_URL}"
echo "  Num Requests:    ${NUM_REQUESTS}"
echo "  Concurrency:     ${CONCURRENCY_LIMIT}"
echo ""
echo "Prerequisites:"
echo "  - Make sure the metrics server is running"
echo "  - Run an example: cargo run --example long_running --features hotpath"
echo ""
echo "Starting load test..."
echo ""

oha -n "${NUM_REQUESTS}" \
    -c "${CONCURRENCY_LIMIT}" \
    -m GET \
    -H "Accept: application/json" \
    "${LOAD_TEST_URL}"

echo ""
echo "Load test completed!"
echo ""
