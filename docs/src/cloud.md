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
