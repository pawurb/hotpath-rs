---
name: hotpath_init_ci_forks
description: Set up the hotpath Cloud CI integration in a public Rust project that benchmarks pull requests from forks. Adds the hotpath-cloud feature, a regression policy file, a GitHub Actions benchmark workflow that writes hotpath reports as artifacts and a relay workflow that uploads them, so every pull request, forks included, gets a performance comment. Use when the user wants to add hotpath Cloud to an open source project or benchmark pull requests from forks.
allowed-tools: Bash, Read, Edit, Write, Glob, Grep
---

# Initialize hotpath Cloud CI for pull requests from forks

Set up [hotpath Cloud](https://hotpath.rs/cloud) in the current Rust project, covering pull requests from forks. A GitHub Actions job runs a benchmark with hotpath profiling enabled. hotpath.rs compares each pull request's report with the default branch's, judges the difference under the repository's policy file and posts a comment on the pull request.

A job triggered by a fork's pull request runs the fork's code, so for security reasons GitHub gives it no OIDC token and it cannot upload. So:

- pushes to the default branch upload directly from the benchmark job;
- pull requests write the report to a file and store it as an artifact;
- a relay workflow, triggered when the benchmark workflow completes, uploads the artifact from a job that never checks out or runs code from the pull request.

This setup also covers pull requests from branches of the same repository. For a private repository, or one that does not need fork pull requests benchmarked, the simpler `hotpath_init_ci` skill is enough (`hotpath init-ci` without `--forks`).

Reference: https://hotpath.rs/ci_integration#pull-requests-from-forks, https://hotpath.rs/regression_policy.

## Steps

### 1. Inspect the project

- Find the crate(s), the default branch (`git symbolic-ref --short refs/remotes/origin/HEAD`, usually `main`) and the existing workflows in `.github/workflows/`.
- Check whether hotpath is already set up: a `hotpath` dependency, `#[hotpath::main]` (or `HotpathGuardBuilder`) and instrumented functions. If it is not, set it up first by following the `hotpath_init` skill (https://raw.githubusercontent.com/pawurb/hotpath-rs/main/skills/hotpath_init/SKILL.md), then continue here.

### 2. Pick the benchmark

The benchmark is a program instrumented with hotpath that runs a **fixed workload**: the same work on every run, so two runs of one commit report the same allocations. It runs to completion and exits normally, so the hotpath guard is dropped and the report is written.

- Prefer an existing example, binary or integration test that exercises the code the user cares about. Ask the user which one when it is not obvious.
- If there is none, propose a new example (`examples/hotpath_benchmark.rs`) that drives the main code paths with fixed inputs, and write it only after the user agrees.
- No randomness without a fixed seed, no network calls to services CI cannot reach, no timing-dependent loops. Enough calls per function that the numbers are stable.
- Pick the benchmark name, used as `HOTPATH_BENCHMARK` and as the relay's `benchmark` input: 1 to 64 characters from `[A-Za-z0-9._-]`, for example the example's name.

### 3. Add the `hotpath-cloud` feature

In the benchmark crate's `Cargo.toml`, next to the existing hotpath features:

```toml
[features]
hotpath = ["hotpath/hotpath"]
hotpath-alloc = ["hotpath/hotpath-alloc"]
hotpath-cloud = ["hotpath/hotpath-cloud"]
```

Never add it to `default`. The pull request job needs it too, although it does not upload: the feature is what adds the commit, the pull request, the benchmark name and the policy file to the JSON report.

### 4. Add the policy file

```bash
mkdir -p hotpath
curl -fsSL https://hotpath.rs/api/v1/policy/default | jq -j .source > hotpath/policy.toml
```

Run it from the repository root. It writes the default policy, with comments, to `hotpath/policy.toml` (no token needed; with `wget`, use `wget -qO- https://hotpath.rs/api/v1/policy/default`). Never overwrite an existing policy file. Every upload must carry a policy file, so commit it. Do not change the defaults unless the user asks; https://hotpath.rs/regression_policy documents every key.

After any edit, validate the file. A valid policy answers `{"valid":true}`, an invalid one HTTP 422 with `problems`, each with its `line` and `message`:

```bash
jq -Rs '{source: .}' hotpath/policy.toml \
  | curl -sS -X POST https://hotpath.rs/api/v1/policy/validate -H 'Content-Type: application/json' --data-binary @-
```

A pull request is judged under the policy file of its own branch, forks included. Tell the user to review changes to `hotpath/*.toml` in pull requests like changes to CI configuration.

### 5. Add the benchmark workflow

Create `.github/workflows/hotpath-benchmark.yml`. Replace `main` with the default branch, `my_benchmark` with the benchmark name, and both `cargo run` lines with the command that runs the benchmark (`-p <crate>` in a workspace, `--bin` or `--example` as appropriate). Keep `--release`. Drop `hotpath-alloc` from `--features` only if the crate has no such feature.

```yaml
name: hotpath benchmark

on:
  push:
    branches: [main]
  pull_request:

jobs:
  upload:
    name: hotpath benchmark (push)
    if: github.event_name == 'push'
    runs-on: ubuntu-latest
    timeout-minutes: 20
    permissions:
      contents: read
      id-token: write
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - name: Run benchmark and upload report
        env:
          HOTPATH_UPLOAD: fail-on-error
          HOTPATH_BENCHMARK: my_benchmark
          HOTPATH_USER_METADATA: runner=${{ runner.os }}-${{ runner.arch }},toolchain=stable,profile=release
          HOTPATH_OUTPUT_FORMAT: none
        run: |
          set -o pipefail
          cargo run -q --release --example my_benchmark \
            --features hotpath,hotpath-alloc,hotpath-cloud 2>&1 | tee run.log
          grep -q '^hotpath: uploaded report ' run.log

  report:
    name: hotpath benchmark (pull request)
    if: github.event_name == 'pull_request'
    runs-on: ubuntu-latest
    timeout-minutes: 20
    permissions:
      contents: read
    steps:
      - uses: actions/checkout@v4
        with:
          # The pull request's head commit, not the default merge commit.
          ref: ${{ github.event.pull_request.head.sha }}
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - name: Run benchmark and write report
        env:
          HOTPATH_BENCHMARK: my_benchmark
          HOTPATH_USER_METADATA: runner=${{ runner.os }}-${{ runner.arch }},toolchain=stable,profile=release
          HOTPATH_OUTPUT_FORMAT: json
          HOTPATH_OUTPUT_PATH: report.json
        run: |
          cargo run -q --release --example my_benchmark \
            --features hotpath,hotpath-alloc,hotpath-cloud
          test -s report.json
      - uses: actions/upload-artifact@v4
        with:
          name: hotpath-report
          path: report.json
          retention-days: 1
          if-no-files-found: error
```

- Add any setup the benchmark needs (system packages, a database service) to both jobs, before the benchmark, copied from the repository's existing CI.
- Never set `HOTPATH_UPLOAD` or `id-token: write` in the `report` job: it runs untrusted code.

### 6. Add the relay workflow

Create `.github/workflows/hotpath-relay.yml`. The `workflows:` entry must be exactly the `name:` of the benchmark workflow above.

```yaml
name: hotpath relay

on:
  workflow_run:
    workflows: ["hotpath benchmark"]
    types: [completed]

jobs:
  relay:
    uses: pawurb/hotpath-rs/.github/workflows/hotpath-relay.yml@main
    permissions:
      actions: read # download the artifact of the benchmark run
      id-token: write # authenticate the upload
      pull-requests: read # find the pull request the run belongs to
    with:
      benchmark: my_benchmark
      artifact_name: hotpath-report
      report_file: report.json
```

Add `fail_on_regression: true` under `with:` only if the user wants a regression or a broken budget to fail the relay run. Tell the user:

- `workflow_run` runs the relay from the **default branch** only, so it does nothing until it is merged.
- Renaming the benchmark workflow silently stops the relay.
- The relay run is not a check on the pull request; its result shows in the Actions tab and in the pull request comment.

### 7. Verify locally

```bash
cargo check                                                    # feature off still compiles
HOTPATH_BENCHMARK=my_benchmark HOTPATH_OUTPUT_FORMAT=json HOTPATH_OUTPUT_PATH=/tmp/hotpath-report.json \
  cargo run --release --example my_benchmark --features hotpath,hotpath-alloc,hotpath-cloud
jq '{benchmark: .meta.benchmark, policy: .meta.policy.path}' /tmp/hotpath-report.json
```

The report must name the benchmark and `hotpath/policy.toml`: this is the file the pull request job stores as the artifact. Run the benchmark twice and compare the function allocation totals: if they differ, the workload is not fixed, so fix that before relying on the comments.

### 8. Tell the user what is left

- Install the [hotpath-rs GitHub App](https://github.com/apps/hotpath-rs) on the repository. It needs no access to the code, only pull request write access to post the comment.
- Commit both workflows and merge them to the default branch. The first push to the default branch uploads the **baseline**, and the relay only runs once it is on the default branch. Until the baseline exists, pull request comments read "no baseline yet".
- From then on, every pull request, forks included, gets a performance comment. The `hotpath_cloud` skill and `hotpath cloud diff` read the same verdict as JSON.

Do not commit or push unless the user asks.

## Rules

- Never enable `hotpath`, `hotpath-alloc` or `hotpath-cloud` by default; profiling stays opt-in.
- Never put a token or secret in either workflow; uploads authenticate with the OIDC token of trusted jobs.
- Never give the pull request job `id-token: write` or secrets, and never check out pull request code in the relay.
- One benchmark per `HOTPATH_BENCHMARK` name. For several benchmarks, add one job pair and one relay job each, with their own names and artifact names; a benchmark can have its own policy in `hotpath/<benchmark>-policy.toml` (fetched the same way as `hotpath/policy.toml`).
- Keep the edits to the feature, the policy file, the two workflows and, if the user agreed, the benchmark program.
