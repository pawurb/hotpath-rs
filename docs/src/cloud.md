# `hotpath Cloud` - performance feedback for developers and coding agents

> **Note:** hotpath Cloud is currently in closed beta.

`hotpath Cloud` is a backend service for the open-source `hotpath-rs` profiler. It provides an automated feedback loop for performance signals collected from your Rust application, helping both developers and coding agents detect regressions before they are merged.

The [GitHub Actions integration](ci_integration) compares benchmark results against a baseline and posts a PR comment whenever a significant regression or improvement is detected:

<img loading="lazy" src="{{#asset-hash images/cloud-pr-comment.webp}}" alt="hotpath Cloud pull request comment">

You can define acceptable regression thresholds using a customizable [regression policy](/regression_policy) and set fine-tuned [performance budgets](/performance_budgets). This helps eliminate noisy alerts and ensures that warnings are emitted only when performance changes in a meaningful way.

The [`hotpath cloud`](agents_cli) CLI and dedicated AI skill bring the same feedback loop to coding agents. Agents can analyze benchmark results, identify regressions, and use the profiling data to guide performance optimization:

<img loading="lazy" src="{{#asset-hash images/cloud-agent-diff.webp}}" alt="AI agent summarizing a hotpath Cloud diff">

Benchmark reports are persisted in `hotpath Cloud`, making it easy to share results with your team and compare each report against its baseline:

<img loading="lazy" src="{{#asset-hash images/cloud-dashboard-diff.webp}}" alt="hotpath Cloud dashboard diff page">

[See this diff on hotpath.rs](https://hotpath.rs/app/repos/pawurb/hotpath-rs/benchmarks/drain/reports/01a0f476-5b3a-7d0e-901a-7756f28e2414/diff)

## Getting Started

### AI Setup (Recommended)

The quickest way to set up `hotpath Cloud` is to let your own AI coding agent do it. Install the `hotpath` CLI and run `init-ci` inside your project repo:

```bash
cargo install hotpath --version '^{{HOTPATH_VERSION}}'
hotpath init-ci --agent claude # or --agent codex / --agent opencode
```

`hotpath init-ci` downloads the [hotpath_init_ci agent skill](https://github.com/pawurb/hotpath-rs/blob/main/skills/hotpath_init_ci/SKILL.md) from GitHub and starts your installed Claude Code, Codex or OpenCode with it as setup instructions. The agent adds the `hotpath-cloud` feature, a [regression policy](regression_policy.md) file and a GitHub Actions benchmark workflow, so every pull request gets a performance comment.

Use `--forks` for public repositories that want to benchmark pull requests from forks. Fork pull requests need a [separate setup](ci_integration.md#pull-requests-from-forks) for security reasons:

```bash
hotpath init-ci --agent claude --forks
```

For private repositories, or if you don't need benchmarks on pull requests from forks, run `hotpath init-ci` without `--forks`.

Your agent remains in control: you review and approve edits through its regular permission prompts. Requires `curl` and the `claude`, `codex` or `opencode` CLI on `PATH`.

### Manual setup

Follow the [CI integration](ci_integration.md) guide to add the benchmark workflow by hand.
