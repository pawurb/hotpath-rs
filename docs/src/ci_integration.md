# CI integration: benchmark Rust pull requests in GitHub Actions

> **Note:** hotpath Cloud is currently in closed beta.

`hotpath` reviews the performance of every pull request. Your CI job runs a benchmark with `hotpath` profiling enabled and uploads the report. hotpath.rs compares it with the report of the base branch, judges the difference under your policy and posts the result as a comment on the pull request.

What the comment reports is up to you:

- which resources are compared: functions (time and allocations), SQL queries, outbound HTTP calls, served routes, locks, channels and I/O
- what counts as a regression: the [regression policy](regression_policy.md) sets the bar per metric family, the metrics that decide, and the entries to ignore
- hard limits that hold whatever the base branch did: [performance budgets](performance_budgets.md), for example "this function must never allocate"
- whether a regression only shows in the comment or also fails the CI job

The upload authenticates with the GitHub Actions OIDC token of the job, so there is no secret to create or store.

## Setup

Install the [hotpath-rs GitHub App](https://github.com/apps/hotpath-rs) on the repository. It is what posts the comments.

Add the `hotpath-cloud` feature next to the profiling features you use:

```toml
[dependencies]
hotpath = "{{HOTPATH_VERSION}}"

[features]
hotpath = ["hotpath/hotpath"]
hotpath-alloc = ["hotpath/hotpath-alloc"]
hotpath-cloud = ["hotpath/hotpath-cloud"]
```

Like the rest of `hotpath`, the uploader is compiled only when its feature is enabled. At runtime it does nothing unless `HOTPATH_UPLOAD=1` is set.

The benchmark is any program of yours instrumented with `hotpath`: an example, a binary or an integration test that runs a fixed workload. The report is uploaded when the `hotpath` guard is dropped, at the end of the run.

There are two ways to wire the workflow, depending on where pull requests come from:

| Pull requests come from | Setup |
|---|---|
| branches of the same repository | [one workflow](#pull-requests-from-the-same-repository) that uploads from the benchmark job |
| forks (open source projects) | [two workflows](#pull-requests-from-forks): the benchmark job writes the report, a relay job uploads it |

The fork setup also handles pull requests from the same repository, so a project that accepts both needs only that one.

## Pull requests from the same repository

One workflow runs on pushes to the default branch and on pull requests. Both upload from the benchmark process itself.

```yaml
name: Benchmark

on:
  push:
    branches: [main]
  pull_request:

jobs:
  benchmark:
    runs-on: ubuntu-latest
    timeout-minutes: 20
    permissions:
      contents: read
      id-token: write # the upload authenticates with the job's OIDC token
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - name: Run benchmark and upload report
        env:
          HOTPATH_UPLOAD: "1"
          HOTPATH_BENCHMARK: my_benchmark
          HOTPATH_OUTPUT_FORMAT: none
          # Fail the job when the upload itself fails:
          # HOTPATH_UPLOAD_STRICT: "1"
          # Fail the job on a regression or a broken budget:
          # HOTPATH_UPLOAD_FAIL_ON_REGRESSION: "1"
        run: |
          cargo run --release --example my_benchmark \
            --features hotpath,hotpath-alloc,hotpath-cloud
```

Why both triggers: the report of a push to the default branch is the **baseline** that pull requests are compared against. A pull request is compared with the report of the commit it branched from, or with the newest report of its base branch when that commit has none (the comment says so). Until the default branch has a report, comments read "no baseline yet".

The job prints one line when it finishes:

```
hotpath: uploaded report 01a0e1d7-... (repository owner/repo, benchmark my_benchmark, baseline 01a0dfb3-...); verdict: no regressions
```

Under GitHub Actions the same line is an annotation on the run, and the step summary carries the server's response with a link to the report on hotpath.rs.

Pull requests from forks and from Dependabot trigger this workflow too, but GitHub gives their jobs no OIDC token. The upload is then skipped with a notice and the job passes. To cover them, use the fork setup below.

### Failing the job on a regression

By default a regression shows in the pull request comment and as a warning on the run, and the job passes. Two variables turn the upload into a CI guard:

- `HOTPATH_UPLOAD_FAIL_ON_REGRESSION=1` makes the benchmark process exit with code 1 when the verdict is a regression or a broken budget. A report nothing could be judged on (no baseline yet and no budgets) never fails the job.
- `HOTPATH_UPLOAD_RESPONSE_PATH=upload.json` writes the server's response to a file, for rules of your own:

```yaml
      - name: Run benchmark and upload report
        env:
          HOTPATH_UPLOAD: "1"
          HOTPATH_BENCHMARK: my_benchmark
          HOTPATH_UPLOAD_RESPONSE_PATH: upload.json
        run: cargo run --release --example my_benchmark --features hotpath,hotpath-cloud
      - name: Fail on more than two regressions
        run: jq -e '.verdict.regressions <= 2' upload.json
```

The response file looks like this:

```json
{
  "id": "01a0e1d7-6b90-7785-8657-16032ad1dd8f",
  "repository": "owner/repo",
  "benchmark": "my_benchmark",
  "baseline": "01a0dfb3-def1-738b-a171-c46b4fa57601",
  "comment": { "url": "https://github.com/owner/repo/pull/12#issuecomment-1" },
  "verdict": {
    "judged": true,
    "regressed": true,
    "regressions": 2,
    "improvements": 1,
    "budgets_broken": 0
  },
  "dashboard_url": "https://hotpath.rs/app/repos/owner/repo/benchmarks/my_benchmark/reports/01a0e1d7-6b90-7785-8657-16032ad1dd8f/diff"
}
```

The file is written only when the report was uploaded. A failed or skipped upload has no verdict and leaves no file.

The verdict is the one the pull request comment shows, judged under the policy in force at the time of the upload.

### Environment variables

| Variable | Description |
|---|---|
| `HOTPATH_UPLOAD` | Set to `1` to upload the report when the run ends. Without it nothing is sent. (default: off) |
| `HOTPATH_BENCHMARK` | Name of the benchmark series, 1 to 64 characters from `[A-Za-z0-9._-]`. Reports are compared within one series, and every series gets a pull request comment of its own. An invalid name skips the upload. (default: `default`) |
| `HOTPATH_UPLOAD_STRICT` | Set to `1` to exit with code 1 when the upload fails (server unreachable, report rejected). A skipped upload never fails. (default: off, a failed upload is a warning) |
| `HOTPATH_UPLOAD_FAIL_ON_REGRESSION` | Set to `1` to exit with code 1 when the verdict is a regression or a broken budget. Independent of `HOTPATH_UPLOAD_STRICT`. (default: off) |
| `HOTPATH_UPLOAD_RESPONSE_PATH` | File the server's response is written to as JSON. (default: not written) |
| `HOTPATH_API_URL` | Base URL of the server. (default: `https://hotpath.rs`) |
| `HOTPATH_USER_METADATA` | Comma-separated `key=value` pairs stored with the report and shown on its page, e.g. `runner=linux-x64,profile=release`. |
| `HOTPATH_SOURCE_ROOT` | Path of the build workspace relative to the repository root, for source links in the comment. Derived from the checkout when unset. |
| `HOTPATH_OUTPUT_FORMAT` | Set to `none` to keep the local report out of the job log. The upload does not depend on it. |

The uploaded report always carries every entry of every section. `HOTPATH_LIMIT` and the per-section limits do not apply to it, because a report cut to the top entries cannot be compared entry by entry.

See [configuration](configuration.md) for the variables that shape the report itself.

## Pull requests from forks

A job triggered by a pull request from a fork runs the fork's code, so GitHub gives it a read-only token and no OIDC token, whatever the workflow asks for. The benchmark job cannot upload. It writes the report to an artifact instead, and a second workflow, triggered when the first one completes, uploads it from a job that never checks out or runs code from the pull request.

### The benchmark workflow

Pushes to the default branch upload directly, as before. Pull requests write the report to a file and store it as an artifact.

```yaml
name: Benchmark

on:
  push:
    branches: [main]
  pull_request:

jobs:
  upload:
    name: benchmark (push)
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
          HOTPATH_UPLOAD: "1"
          HOTPATH_BENCHMARK: my_benchmark
          HOTPATH_OUTPUT_FORMAT: none
        run: |
          cargo run --release --example my_benchmark \
            --features hotpath,hotpath-alloc,hotpath-cloud

  report:
    name: benchmark (pull request)
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
          HOTPATH_OUTPUT_FORMAT: json
          HOTPATH_OUTPUT_PATH: report.json
        run: |
          cargo run --release --example my_benchmark \
            --features hotpath,hotpath-alloc,hotpath-cloud
          test -s report.json
      - uses: actions/upload-artifact@v4
        with:
          name: hotpath-report
          path: report.json
          retention-days: 1
          if-no-files-found: error
```

The pull request job still builds with the `hotpath-cloud` feature. The feature is what adds the commit, the pull request and the benchmark name to the JSON report. A report built without it is rejected by the relay. `HOTPATH_UPLOAD` is not set in that job: it has no token to upload with.

### The relay workflow

```yaml
name: Benchmark relay

on:
  workflow_run:
    workflows: ["Benchmark"] # the `name:` of the benchmark workflow
    types: [completed]

jobs:
  relay:
    uses: pawurb/hotpath-rs/.github/workflows/hotpath-relay.yml@main
    permissions:
      actions: read # download the artifact of the benchmark run
      contents: read
      id-token: write # authenticate the upload
    with:
      benchmark: my_benchmark
      artifact_name: hotpath-report
      report_file: report.json
      # fail_on_regression: true
```

`hotpath-relay.yml` is a reusable workflow. It downloads the artifact, checks that the report describes the pull request the run belongs to, uploads it and prints the server's response in the step summary.

| Input | Description |
|---|---|
| `benchmark` | Name of the benchmark series, as in `HOTPATH_BENCHMARK`. Required. |
| `artifact_name` | Name of the artifact the benchmark job uploaded. (default: `hotpath-report`) |
| `report_file` | Name of the JSON report inside the artifact. (default: `report.json`) |
| `source_root` | Path of the build workspace relative to the repository root, `""` when they are the same. (default: `""`) |
| `fail_on_regression` | Fail the relay job when the verdict is a regression or a broken budget. (default: `false`) |
| `upload_url` | Base URL of the server. (default: `https://hotpath.rs`) |

Things to know about `workflow_run`:

- It runs the copy of the relay workflow that is on the **default branch**. Changes to it do nothing until they are merged.
- It matches the benchmark workflow by **name**. Rename that workflow and the relay stops firing, without an error.
- Its jobs are **not checks on the pull request**. With `fail_on_regression: true` a regression fails the relay run, which shows in the Actions tab and in the pull request comment, not among the pull request's checks.

### Environment variables

In the pull request job, which writes the report:

| Variable | Description |
|---|---|
| `HOTPATH_BENCHMARK` | Name of the benchmark series. The relay's `benchmark` input must name the same one. |
| `HOTPATH_OUTPUT_FORMAT` | Must be `json`. |
| `HOTPATH_OUTPUT_PATH` | File the report is written to, the one stored as the artifact. |
| `HOTPATH_USER_METADATA` | Comma-separated `key=value` pairs stored with the report. |

The push job takes the variables of the [same-repository setup](#environment-variables).

## Several benchmarks in one repository

Give every benchmark its own `HOTPATH_BENCHMARK` name and its own job. Each series has its own baseline, its own policy when you set one, and its own comment on the pull request.

## Troubleshooting

| What you see | Cause |
|---|---|
| `upload skipped` in the job log | The job has no OIDC token: `id-token: write` is missing, or the pull request comes from a fork or from Dependabot. |
| `upload failed: ... (HTTP 403)` | The GitHub App is not installed on the repository. |
| `upload failed: ... (HTTP 429)` | The repository is over its upload quota. The message says when to retry. |
| The comment reads "no baseline yet" | The base branch has no report in this series. It appears after the first push to the default branch with the workflow in place. |
| The comment reads "report not readable" | The report was produced by a `hotpath` version the server no longer reads. Update `hotpath`. |
| The report is uploaded but no comment appears | The response's `comment.error` says why, and the job shows it as a warning. The usual cause is that the App's "Pull requests: write" permission is not approved for the installation. |
| The relay does not run | The `workflows:` name does not match the benchmark workflow's `name:`, or the relay workflow is not on the default branch yet. |
