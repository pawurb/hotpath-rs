# GitHub CI: automated performance benchmarking and regression detection

Hotpath includes a `hotpath-utils` CLI that compares performance metrics between a PR branch and its base, then posts a profiling diff as a PR comment. This lets you catch runtime regressions before merging.

<img loading="lazy" src="{{#asset-hash images/mevlog-enable-cache.png}}" alt="Hotpath CI PR comment showing performance comparison">

## How it works

The integration uses two GitHub Actions workflows:

1. **Profile workflow** (`hotpath-profile`) - triggers on `pull_request`, runs your benchmarks on both the head and base commits, and uploads the metrics as an artifact.
2. **Comment workflow** (`hotpath-comment`) - triggers when the profile workflow completes, downloads the artifact, installs `hotpath-utils`, and posts a comparison comment on the PR.

The two-workflow split is required for security because `pull_request` workflows from forks run with read-only permissions. The second workflow runs in the repository's context with `pull-requests: write` access to enable commenting.

## Setup

### 1. Create benchmark examples

Add benchmark examples to your crate that exercise the functions you want to track:

```rust
#[hotpath::main]
fn main() {
    for _ in 0..1000 {
        my_function();
    }
}
```

### 2. Add the profile workflow

Create `.github/workflows/hotpath-profile.yml`:

```yaml
name: hotpath-profile

on:
  pull_request:

permissions:
  contents: read

jobs:
  profile:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0

      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2

      - id: head_timing
        env:
          HOTPATH_OUTPUT_FORMAT: json
        run: |
          {
            echo 'metrics<<EOF'
            cargo run --release --example my_benchmark \
              --features='hotpath' | grep '^{"hotpath_profiling_mode"'
            echo 'EOF'
          } >> "$GITHUB_OUTPUT"

      - name: Checkout base
        run: git checkout ${{ github.event.pull_request.base.sha }}

      - id: base_timing
        env:
          HOTPATH_OUTPUT_FORMAT: json
        run: |
          {
            echo 'metrics<<EOF'
            cargo run --release --example my_benchmark \
              --features='hotpath' | grep '^{"hotpath_profiling_mode"'
            echo 'EOF'
          } >> "$GITHUB_OUTPUT"

      - name: Save metrics to artifact
        run: |
          mkdir -p /tmp/metrics
          echo '${{ steps.head_timing.outputs.metrics }}' \
            > /tmp/metrics/head_timing.json
          echo '${{ steps.base_timing.outputs.metrics }}' \
            > /tmp/metrics/base_timing.json
          echo '${{ github.event.pull_request.number }}' \
            > /tmp/metrics/pr_number.txt
          echo '${{ github.base_ref }}' > /tmp/metrics/base_ref.txt
          echo '${{ github.head_ref }}' > /tmp/metrics/head_ref.txt

      - uses: actions/upload-artifact@v4
        with:
          name: profile-metrics
          path: /tmp/metrics/
          retention-days: 1
```

`HOTPATH_OUTPUT_FORMAT=json` makes hotpath output metrics as a JSON line. The `grep` extracts that line from any other program output.

### 3. Add the comment workflow

Create `.github/workflows/hotpath-comment.yml`:

```yaml
name: hotpath-comment

on:
  workflow_run:
    workflows: ["hotpath-profile"]
    types:
      - completed

permissions:
  contents: read
  pull-requests: write

jobs:
  comment:
    runs-on: ubuntu-latest
    if: ${{ github.event.workflow_run.conclusion == 'success' }}
    steps:
      - uses: actions/checkout@v4

      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2

      - uses: actions/download-artifact@v4
        with:
          name: profile-metrics
          path: /tmp/metrics/
          github-token: ${{ secrets.GITHUB_TOKEN }}
          run-id: ${{ github.event.workflow_run.id }}

      - name: Install hotpath-utils CLI
        run: cargo install --path crates/hotpath \
          --bin hotpath-utils --features=utils

      - name: Post PR comment
        env:
          GH_TOKEN: ${{ github.token }}
        run: |
          set -euo pipefail
          HEAD=$(cat /tmp/metrics/head_timing.json)
          BASE=$(cat /tmp/metrics/base_timing.json)
          PR=$(cat /tmp/metrics/pr_number.txt)
          export GITHUB_BASE_REF=$(cat /tmp/metrics/base_ref.txt)
          export GITHUB_HEAD_REF=$(cat /tmp/metrics/head_ref.txt)
          hotpath-utils profile-pr \
            --head-metrics "$HEAD" \
            --base-metrics "$BASE" \
            --github-token "$GH_TOKEN" \
            --pr-number "$PR" \
            --benchmark-id "timing"
```

## `hotpath-utils profile-pr` CLI

| Flag | Required | Description |
|------|----------|-------------|
| `--head-metrics` | yes | JSON metrics from the PR branch |
| `--base-metrics` | yes | JSON metrics from the base branch |
| `--github-token` | yes | GitHub token for API access |
| `--pr-number` | yes | Pull request number |
| `--benchmark-id` | no | Unique ID to prevent comment collisions when running multiple benchmarks |
| `--emoji-threshold` | no | % change threshold for warning/celebration emoji (default: 20, 0 to disable) |

The CLI compares functions between the two snapshots and generates a markdown table with:
- Per-function diffs for calls, avg latency, p99, and total time
- Emoji indicators for significant changes (⚠️ regressions, 🚀 improvements)
- 🆕 for new functions and 🗑️ for removed functions
- Collapsible raw JSON details

## Multiple benchmarks

You can run several benchmarks in the same workflow by adding more step pairs (head + base). Use distinct `--benchmark-id` values so each benchmark gets its own PR comment:

```yaml
- id: head_timing
  env:
    HOTPATH_OUTPUT_FORMAT: json
  run: |
    {
      echo 'metrics<<EOF'
      cargo run --release --example benchmark_noop \
        --features='hotpath' | grep '^{"hotpath_profiling_mode"'
      echo 'EOF'
    } >> "$GITHUB_OUTPUT"

- id: head_alloc
  env:
    HOTPATH_OUTPUT_FORMAT: json
  run: |
    {
      echo 'metrics<<EOF'
      cargo run --release --example benchmark_alloc \
        --features='hotpath,hotpath-alloc' | grep '^{"hotpath_profiling_mode"'
      echo 'EOF'
    } >> "$GITHUB_OUTPUT"
```

Then in the comment workflow, post each with a different `--benchmark-id`:

```bash
hotpath-utils profile-pr \
  --benchmark-id "timing" ...

hotpath-utils profile-pr \
  --benchmark-id "alloc" ...
```
