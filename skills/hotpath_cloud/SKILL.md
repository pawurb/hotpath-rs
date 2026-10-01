---
name: hotpath_cloud
description: Read hotpath Cloud performance verdicts with the `hotpath cloud` CLI and fix Rust performance regressions. Fetches the benchmark report of a commit or pull request, compares it with its baseline under the repository's regression policy and performance budgets, and lists what regressed and which budgets broke, as JSON. Use when the user wants to check a pull request's performance, wait for a CI benchmark, investigate a hotpath.rs PR comment, fix a performance regression or a broken budget, or validate a hotpath policy file.
allowed-tools: Bash, Read, Edit, Write, Glob, Grep
---

# hotpath Cloud CLI

`hotpath cloud` is the command line client of hotpath.rs. CI uploads a hotpath benchmark report on every push to the default branch and every pull request. hotpath.rs compares each pull request report with its baseline (the report of the base branch), judges it under the repository's policy file and posts a comment on the pull request. The CLI returns the same judgement as JSON.

Docs: https://hotpath.rs/agents_cli, https://hotpath.rs/regression_policy, https://hotpath.rs/performance_budgets.

## Setup

1. Check the CLI: `hotpath cloud --help`. If it is missing or has no `cloud` command, install it with `cargo install hotpath --features cloud`.
2. Check the token: `hotpath cloud auth`. It reads `HOTPATH_API_TOKEN`. If it fails with `invalid_token` or the variable is unset, ask the user to create a token at https://hotpath.rs/app/tokens and export it. Never ask the user to paste the token into the conversation.
3. Find the repository (`OWNER/NAME`, from `git remote get-url origin`) and the benchmark name: `hotpath cloud benchmarks --repo OWNER/NAME`. The benchmark is also the `HOTPATH_BENCHMARK` value in the repository's `.github/workflows/*.yml`.

`validate-policy` and `init` need no token.

## Output contract

- Every command prints one JSON document on stdout (`--pretty` to indent, `--output FILE` to write a file).
- An error prints one JSON document on stderr, like `{"error":"...","code":"not_found"}`, and nothing on stdout.
- Exit codes: `0` success, `1` error, `2` usage error. For `diff`, `0` means exactly "judged and nothing regressed"; anything else is `1` with the answer still on stdout.

Branch on the exit code, then read the JSON. Do not parse the human text of the pull request comment.

## Selecting a report

`report` and `diff` take `--repo OWNER/NAME --benchmark NAME` and one of:

- `--commit SHA`: a full 40-character sha. It matches the measured commit or a pull request's head commit, so `--commit $(git rev-parse HEAD)` works on a pull request branch.
- `--pr N`: the newest report of the pull request.
- `--id ID`: one report.

`--event push|pull_request` narrows `--pr` or `--commit` to one event.

## Workflow: check or fix a change

1. Push the commit. CI runs the benchmark and uploads the report, which takes as long as the benchmark job.
2. Wait for the report. Poll until it exits `0`, waiting about 30 seconds between attempts and giving up after about 30 minutes:

   ```bash
   hotpath cloud report --repo OWNER/NAME --benchmark NAME --commit $(git rev-parse HEAD) --no-payload
   ```

   `not_found` means the upload has not arrived yet. If it never arrives, check the CI run with `gh run list` / `gh run view`.
3. Read the judgement:

   ```bash
   hotpath cloud diff --repo OWNER/NAME --benchmark NAME --commit $(git rev-parse HEAD)
   ```

4. Exit `0`: done, report the verdict to the user. Exit `1`: read the regressions and broken budgets (below), fix the code, and repeat from step 1.

## Reading `diff`

- `verdict`: `judged` (anything was judged), `regressed` (a judged section regressed or a budget broke), `regressions`, `improvements`, `budgets_broken`.
- `result.status`: `compared`, `no_baseline` (the base branch has no report yet; only budgets are judged) or `unreadable` (a report or its policy cannot be read; nothing is judged).
- `result.sections[]`: one per section of the policy (`resource` + `kind`, e.g. `functions` + `alloc`). `family.judged` says whether it counts toward the verdict, `family.metrics` which columns it judges, `family.counts` how many rows had each outcome.
- `rows[]`: `name`, `location` (`file`, `line` of the function in the measured commit), `outcome` (`regression`, `improvement`, `added`, `removed`) and `cells`.
- `cells[]`: `column`, `unit`, `base`, `head` (formatted strings like `"24 B"`, `"2.06 ms"`), `change_percent` (a number, the exact figure to reason about) and `crossed` (`up` / `down`) on the cells that crossed the policy's bar.
- `budgets.findings[]`: broken budget checks, with `entity.name`, `actual`, `limit`, `bound` (`max` or `min`) and the rule's `message`. `budgets.notes` says what could not be checked.
- `head.policy_path`: the policy file the report was judged under.
- `dashboard_url`: the comparison page, to give to the user.

By default `diff` lists only what the pull request comment lists. Add `--advisory` to see findings of sections the policy lists but does not judge (for example timing on shared runners), and `--full` for every row and value.

Useful extracts:

```bash
# Regressions with the values that crossed the bar
hotpath cloud diff ... | jq '.result.sections[]? | .rows[] | select(.outcome == "regression")
  | {name, location, cells: [.cells[] | select(.crossed) | {column, base, head, change_percent}]}'

# Broken budgets with their messages
hotpath cloud diff ... | jq '.budgets.findings[] | {entity: .entity.name, actual, limit, message}'
```

## Fixing a regression

- Start from `location` of each regressed row and the `crossed` cells: an allocation regression points at new or larger allocations in that function, a `calls` / `count` change at extra work, `sql_per_request` at an N+1 query.
- Allocations are deterministic for a fixed workload, so an allocation change is real. Timing on shared CI runners is noisy; be careful attributing a small timing change to the code.
- Profile locally to confirm before and after a fix: run the benchmark the CI workflow runs, with the same `--features`, and compare the printed hotpath report.
- A broken budget names its rule's `message`, which usually says what the limit protects.

## Rules

- Never loosen the policy (`hotpath/*.toml`) or a budget to make a regression or a broken budget go away. Fix the code, or explain the trade-off to the user and let them decide.
- If the user asks to change the policy, edit `hotpath/policy.toml` (or `hotpath/<benchmark>-policy.toml`) and check it with `hotpath cloud validate-policy` (`--benchmark NAME` for a benchmark's own file) before pushing. An upload with an invalid policy is refused.
- `hotpath cloud init` writes the default policy when the repository has none. It never replaces an existing file without `--force`; do not pass `--force` unless the user asks.
- `HOTPATH_API_TOKEN` is a credential: never print it, echo it, log it or commit it.
