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

      - name: Create metrics directory
        run: mkdir -p /tmp/metrics

      - name: Head benchmark (timing)
        env:
          HOTPATH_OUTPUT_FORMAT: json
          HOTPATH_OUTPUT_PATH: /tmp/metrics/head_timing.json
        run: cargo run --release --example my_benchmark
          --features='hotpath'

      - name: Checkout base
        run: git checkout ${{ github.event.pull_request.base.sha }}

      - name: Base benchmark (timing)
        env:
          HOTPATH_OUTPUT_FORMAT: json
          HOTPATH_OUTPUT_PATH: /tmp/metrics/base_timing.json
        run: cargo run --release --example my_benchmark
          --features='hotpath'

      - name: Save PR metadata
        run: |
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

`HOTPATH_OUTPUT_FORMAT=json` makes hotpath output metrics as JSON. `HOTPATH_OUTPUT_PATH` writes the JSON directly to the specified file.

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
          export GITHUB_BASE_REF=$(cat /tmp/metrics/base_ref.txt)
          export GITHUB_HEAD_REF=$(cat /tmp/metrics/head_ref.txt)
          hotpath-utils profile-pr \
            --head-metrics /tmp/metrics/head_timing.json \
            --base-metrics /tmp/metrics/base_timing.json \
            --github-token "$GH_TOKEN" \
            --pr-number "$(cat /tmp/metrics/pr_number.txt)" \
            --benchmark-id "timing"
```

## `hotpath-utils profile-pr` CLI

| Flag | Required | Description |
|------|----------|-------------|
| `--head-metrics` | yes | Path to JSON metrics file from the PR branch |
| `--base-metrics` | yes | Path to JSON metrics file from the base branch |
| `--github-token` | yes | GitHub token for API access |
| `--pr-number` | yes | Pull request number |
| `--benchmark-id` | no | Unique ID to prevent comment collisions when running multiple benchmarks |
| `--emoji-threshold` | no | % change threshold for warning/celebration emoji (default: 20, 0 to disable) |

The CLI automatically compares all available sections (`functions_timing`, `functions_alloc`) between the two reports and generates a markdown comment with:
- Per-function diffs for calls, avg latency, p99, and total time
- Emoji indicators for significant changes (⚠️ regressions, 🚀 improvements)
- 🆕 for new functions and 🗑️ for removed functions

## Multiple benchmarks

You can run several benchmarks in the same workflow by adding more step pairs (head + base). Use distinct `--benchmark-id` values so each benchmark gets its own PR comment:

```yaml
- name: Head noop benchmark (timing)
  env:
    HOTPATH_OUTPUT_FORMAT: json
    HOTPATH_OUTPUT_PATH: /tmp/metrics/head_timing.json
  run: cargo run --release --example benchmark_noop
    --features='hotpath'

- name: Head alloc benchmark (alloc)
  env:
    HOTPATH_OUTPUT_FORMAT: json
    HOTPATH_OUTPUT_PATH: /tmp/metrics/head_alloc.json
  run: cargo run --release --example benchmark_alloc
    --features='hotpath,hotpath-alloc'
```

Then in the comment workflow, post each with a different `--benchmark-id`. The CLI will automatically include all available sections (timing and/or alloc) from each report:

```bash
hotpath-utils profile-pr \
  --benchmark-id "noop" ...

hotpath-utils profile-pr \
  --benchmark-id "alloc" ...
```
