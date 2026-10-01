# Regression policy: decide what counts as a performance regression

> **Note:** hotpath Cloud is currently in closed beta. See [hotpath Cloud](cloud.md) to request access.

The regression policy is a TOML file in your repository that decides which changes between a pull request and its [baseline](ci_integration.md#pull-requests-from-the-same-repository) count as regressions. It names the metrics that are compared, how large a change must be to matter, which entries are too small or too rare to judge, and which ones to ignore. The pull request comment, the comparison page on hotpath.rs and the `hotpath cloud diff` command all read the same judgement from it.

The policy is relative: it compares the pull request with its baseline. For hard limits that hold whatever the baseline did, add [performance budgets](performance_budgets.md) to the same file.

## Policy file

Every uploaded report must carry a policy file. The upload looks for it in this order and takes the first one it finds:

1. the file `HOTPATH_POLICY_PATH` names, relative to the working directory or absolute, inside the repository
2. `hotpath/<benchmark>-policy.toml`, the benchmark's own file
3. `hotpath/policy.toml`, the shared file of every benchmark without its own

The `hotpath/` directory is at the repository root. Without any of these files the upload is refused:

```
hotpath: upload failed: the report carries no policy file and hotpath.rs refuses reports without one. Add `hotpath/my_benchmark-policy.toml` or `hotpath/policy.toml` to the repository (`hotpath cloud init` writes the default policy) and check it with `hotpath cloud validate-policy`.
```

`hotpath cloud init` writes the default policy as `hotpath/policy.toml`, or as the benchmark's own file with `--benchmark NAME`. It needs the [CLI and an API token](agents_cli.md):

```bash
cargo install hotpath --features cloud
hotpath cloud init
```

```json
{"path":"hotpath/policy.toml","replaced":false}
```

An existing file is not replaced unless you pass `--force`. Commit the file. The report carries the policy, so the [fork relay](ci_integration.md#pull-requests-from-forks) needs no extra setup.

### Each report is judged under its own policy

The file is read from the checkout the benchmark ran on, so a pull request is judged under **the policy of its own branch**. A pull request that edits the policy is judged under the edited file. Treat changes to `hotpath/*.toml` like changes to CI configuration and review them. The comment's "Policy" link opens the policy that judged the report.

The policy is stored with the report. The comparison page and `hotpath cloud diff` always judge a report under the policy it carried, so they keep agreeing with the comment that was posted.

## Resources and families

A policy has one table per profiled **resource**. Under each resource there is one table per metric **family**: a group of columns that share a unit and are judged with one bar.

| Family | Judges | `metrics` it accepts |
|---|---|---|
| `functions.alloc` | [function](functions.md) allocations | `avg`, `total`, percentiles (`p95`) |
| `functions.timing` | function timing | `avg`, `total`, percentiles (`p95`) |
| `sql.timing` | [SQL queries](sql_tracing.md) | `avg`, `total`, percentiles (`p95`) |
| `http.timing` | [outbound HTTP requests](http_tracing.md) | `errors`, `avg`, `total`, percentiles (`p95`) |
| `server.alloc` | allocations of [served routes](axum_tracing.md) | `bytes_per_request`, `allocs_per_request`, `avg`, `total`, percentiles (`p95`) |
| `server.timing` | timing of served routes | `status_4xx`, `status_5xx`, `sql_per_request`, `http_per_request`, `avg`, `total`, percentiles (`p95`) |
| `mutexes.timing` | [mutexes](locks.md) | `wait_avg`, `acquire_avg`, percentiles (`wait_p95`, `acquire_p99`) |
| `rw_locks.timing` | [rw locks](locks.md) | `read_wait_avg`, `write_wait_avg`, `read_acquire_avg`, `write_acquire_avg`, percentiles (`read_wait_p95`, ...) |
| `channels.flow` | [channels](data_flow.md) | `sent_per_sec`, `received_per_sec`, `max_queue_size`, `delay_avg`, percentiles (`delay_p95`) |
| `io.flow` | [I/O](io_tracing.md) | `errors`, `avg`, `total`, `bytes_per_sec`, percentiles (`p95`) |

Threads, CPU samples and the Tokio runtime are not compared.

A percentile is written the way hotpath names its column: `p50`, `p95`, `p99.9`. `p95.0` or `p095` are refused. A metric the reports do not contain, such as a percentile they were not profiled with, is skipped with a note.

For most columns a rise is the regression. For `sent_per_sec`, `received_per_sec` and `bytes_per_sec` a drop is the regression: the same work moves slower.

Counts such as `calls`, `count`, `bytes` or `sent_count` describe how much work ran. They are context, not metrics, and a policy that names one in `metrics` is refused. To limit a count, use a [budget](performance_budgets.md).

## Judged, listed and left out

A family is in one of three states, and each state decides where its rows appear:

| | `judged = true` | `judged = false` | family table left out |
|---|---|---|---|
| Pull request comment | findings listed | not listed; shown under "Related metrics" as context for findings of the resource's other family | nowhere |
| Comparison page on hotpath.rs | rows and marks | rows and marks | nowhere |
| `hotpath cloud diff` | findings | nothing | nothing |
| `hotpath cloud diff --advisory` | findings | findings | nothing |
| `hotpath cloud diff --full` | every row | every row | nothing |
| Verdict and exit codes | counts | never counts | never counts |

Use `judged = false` for a family you want to watch without letting it fail a pull request, and leave a family out for a resource you do not want to see at all.

> **Keys fall back to the built-in values, family tables do not.** A key you leave out of a listed family, such as `min_calls`, takes its built-in value. A family table you leave out is not filled in: the family disappears from every view. A file that only contains `[functions.alloc]` judges function allocations and hides SQL, HTTP, routes, locks, channels and I/O. Start from the file `hotpath cloud init` writes, which lists every family, and edit it instead of writing a short file.

## Settings

Each family table takes four keys:

| Key | Meaning |
|---|---|
| `judged` | Whether the family decides the verdict and reaches the comment (see above). |
| `metrics` | The columns that are judged, at least one, none repeated. A row is a regression when any of them crosses the bar in its worse direction. |
| `min_percent_change` | The smallest change, in percent and in either direction, that marks a cell, from `0` to `1000`. `0` marks every change that is not zero. |
| `min_calls` | Rows with fewer calls than this on both sides are not judged ("too few calls"). It counts `calls` for functions, `sent_count` for channels and `count` for the rest. |

Each resource table takes:

| Key | Meaning |
|---|---|
| `ignore` | Patterns of entries that are never judged, at most 64. `*` matches any run of characters, and a pattern must match the whole name. |
| `min_percent_total` | Rows whose share of the run (`% Total`) is below this percent on both sides are skipped as noise, from `0` to `100`. Only `functions`, `sql`, `http` and `server` have it. |
| `budgets` | Absolute limits, see [performance budgets](performance_budgets.md). |

What an `ignore` pattern matches depends on the resource:

| Resource | The pattern matches | Example |
|---|---|---|
| `functions` | the function name | `my_crate::tests::*` |
| `sql`, `http` | `name \| route \| source`, `-` for a missing part | `SELECT * FROM sessions*` matches the query wherever it ran |
| `server` | the route | `GET /health` |
| `mutexes`, `rw_locks`, `channels` | the label | `cache*` |
| `io` | `label \| operation \| -` | `stdout*` matches every operation on it |

## Outcome of a row

Every row of a listed family gets exactly one outcome. The checks run in this order, and the first match wins:

| Outcome | When | In the comment |
|---|---|---|
| ignored | the name matches an `ignore` pattern | no |
| below floor | `% Total` is under `min_percent_total` on both sides | no |
| added | the entry is only in the pull request | yes |
| removed | the entry is only in the baseline | yes |
| too few calls | the count is under `min_calls` on both sides | no |
| regression | a judged metric crossed `min_percent_change` in its worse direction | yes |
| improvement | a judged metric crossed it the other way, and none got worse | yes |
| unchanged | no judged metric crossed it | no |

The floor is checked before added and removed, so a new function below the floor is not reported as new. Added and removed rows are listed but do not count as regressions.

## The default policy

This is the file `hotpath cloud init` writes:

```toml
# hotpath-rs regression policy.
#
# Docs: https://hotpath.rs

[functions]
min_percent_total = 0.5

[functions.alloc]
judged = true
min_percent_change = 20
metrics = ["avg", "total"]
min_calls = 0

[functions.timing]
judged = false
min_percent_change = 50
metrics = ["avg", "total"]
min_calls = 10

[sql]
min_percent_total = 0.5

[sql.timing]
judged = false
min_percent_change = 50
metrics = ["avg", "total"]
min_calls = 0

[http]
min_percent_total = 0.5

[http.timing]
judged = false
min_percent_change = 50
metrics = ["avg", "total"]
min_calls = 0

[server]
min_percent_total = 0.5

[server.alloc]
judged = true
min_percent_change = 2
metrics = ["avg", "total"]
min_calls = 0

[server.timing]
judged = true
min_percent_change = 50
metrics = ["avg", "total"]
min_calls = 0

[mutexes.timing]
judged = false
min_percent_change = 50
metrics = ["wait_avg", "acquire_avg"]
min_calls = 0

[rw_locks.timing]
judged = false
min_percent_change = 50
metrics = ["read_wait_avg", "write_wait_avg", "read_acquire_avg", "write_acquire_avg"]
min_calls = 0

[channels.flow]
judged = false
min_percent_change = 50
metrics = ["delay_avg", "max_queue_size"]
min_calls = 0

[io.flow]
judged = false
min_percent_change = 50
metrics = ["avg", "errors", "bytes_per_sec"]
min_calls = 0
```

Allocations are judged and most timing is not. A benchmark with a fixed workload allocates the same bytes on every run of a commit, so even a small change in allocations is a real change. Timing on a shared CI runner varies from run to run, so by default it is shown on the comparison page but does not decide the verdict. Entries under 0.5% of the run are skipped as noise.

## Recipes

Each recipe changes the file `hotpath cloud init` wrote. The tables that are not shown stay as they are.

### Judge timing on a dedicated runner

On a machine that runs nothing else, timing is stable enough to judge. Judge function timing and SQL with a lower bar:

```toml
[functions.timing]
judged = true
min_percent_change = 15
metrics = ["avg", "total"]
min_calls = 100

[sql.timing]
judged = true
min_percent_change = 20
metrics = ["avg", "total"]
min_calls = 10
```

### Ignore test helpers and setup code

```toml
[functions]
min_percent_total = 0.5
ignore = ["my_crate::test_support::*", "my_crate::setup"]

[sql]
min_percent_total = 0.5
ignore = ["CREATE TABLE*", "INSERT INTO fixtures*"]
```

### Judge tail latency

Judge `p99` next to `avg`, so a change that only slows down the slowest calls is caught too:

```toml
[server.timing]
judged = true
min_percent_change = 30
metrics = ["avg", "p99"]
min_calls = 0
```

### Hide a resource

Delete its family tables. The file below judges allocations of functions and routes, and hides everything else, including the timing that the default only shows as context:

```toml
[functions]
min_percent_total = 0.5

[functions.alloc]
judged = true
min_percent_change = 20
metrics = ["avg", "total"]
min_calls = 0

[server]
min_percent_total = 0.5

[server.alloc]
judged = true
min_percent_change = 2
metrics = ["avg", "total"]
min_calls = 0
```

### A different policy for one benchmark

Write `hotpath/<benchmark>-policy.toml` next to the shared file. A benchmark with its own file is judged under that file alone: nothing is taken from `hotpath/policy.toml`.

```bash
hotpath cloud init --benchmark my_benchmark
```

## Checking a policy

`hotpath cloud validate-policy` sends a policy file to hotpath.rs, which parses it the way an upload would. Without arguments it checks `hotpath/policy.toml`. With `--benchmark NAME` it checks the file a run of that benchmark would carry, and with `--file PATH` any file (`-` reads stdin).

```bash
hotpath cloud validate-policy --benchmark my_benchmark --pretty
```

```json
{
  "path": "hotpath/my_benchmark-policy.toml",
  "valid": true,
  "problems": []
}
```

A refused file lists its problems with their line. The command exits with code 1:

```json
{
  "path": "hotpath/policy.toml",
  "valid": false,
  "problems": [
    {
      "line": 3,
      "message": "unknown field `min_chnage`, expected one of `judged`, `min_percent_change`, `metrics`, `min_calls`"
    }
  ]
}
```

A misspelled key, an unknown family, a column the family does not have and a count named as a metric are all refused this way. Run the command before you push: an upload whose policy does not parse is refused.
