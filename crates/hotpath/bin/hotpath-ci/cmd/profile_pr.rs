mod comment;

use clap::Parser;
use comment::upsert_pr_comment;
use eyre::Result;
use hotpath::format_bytes;
use hotpath::json::{JsonFunctionEntry, JsonFunctionsList};
use prettytable::{Cell, Row, Table};
use std::env;
use std::fmt;
use std::time::Duration;

#[derive(Debug, Parser)]
pub struct ProfilePrArgs {
    #[arg(long, help = "JSON metrics from head branch")]
    head_metrics: String,

    #[arg(long, help = "JSON metrics from base branch")]
    base_metrics: String,

    #[arg(long, help = "GitHub token for API access")]
    github_token: String,

    #[arg(long, help = "Pull request number")]
    pr_number: String,

    #[arg(
        long,
        help = "Emoji threshold percentage for performance changes (default: 20, use 0 to disable)"
    )]
    emoji_threshold: Option<u32>,

    #[arg(
        long,
        help = "Unique identifier for this benchmark to prevent comment collisions"
    )]
    benchmark_id: Option<String>,
}

impl ProfilePrArgs {
    pub fn run(&self) -> Result<()> {
        let repo = env::var("GITHUB_REPOSITORY").unwrap_or_default();

        if repo.is_empty() || self.pr_number.is_empty() {
            println!("No PR context found, skipping comment posting");
            return Ok(());
        }

        // Convert emoji_threshold: None -> Some(20), Some(0) -> None
        let emoji_threshold = if let Some(0) = self.emoji_threshold {
            None
        } else {
            Some(self.emoji_threshold.unwrap_or(20))
        };

        let head_metrics_data: JsonFunctionsList = serde_json::from_str(&self.head_metrics)
            .map_err(|e| eyre::eyre!("Failed to deserialize head metrics: {}", e))?;
        let base_metrics_data: JsonFunctionsList = serde_json::from_str(&self.base_metrics)
            .map_err(|e| eyre::eyre!("Failed to deserialize base metrics: {}", e))?;

        let comparison = compare_metrics(&base_metrics_data, &head_metrics_data);
        let comparison_markdown = format_comparison_markdown(
            &comparison,
            &base_metrics_data,
            emoji_threshold,
            self.benchmark_id.as_deref(),
        );

        let mut body = String::new();
        body.push_str(&comparison_markdown);
        body.push_str("\n<details>\n<summary>📊 View Raw JSON Metrics</summary>\n\n");
        body.push_str("### PR Metrics\n```json\n");
        body.push_str(&serde_json::to_string_pretty(&head_metrics_data)?);
        body.push_str("\n```\n\n### Main Branch Metrics\n```json\n");
        body.push_str(&serde_json::to_string_pretty(&base_metrics_data)?);
        body.push_str("\n```\n</details>\n");

        match upsert_pr_comment(
            &repo,
            &self.pr_number,
            &self.github_token,
            &body,
            &head_metrics_data.hotpath_profiling_mode,
            self.benchmark_id.as_deref(),
        ) {
            Ok(_) => {}
            Err(e) => println!("Failed to post/update comment: {}", e),
        }

        Ok(())
    }
}

#[derive(Debug, Clone)]
pub enum MetricDiff {
    CallsCount(u64, u64), // (before, after)
    DurationNs(u64, u64), // (before, after) - Duration in nanoseconds
    Alloc(u64, u64),      // (before, after) - Bytes allocated
    Percentage(u64, u64), // (before, after)
}

impl fmt::Display for MetricDiff {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.format_with_emoji(None))
    }
}

impl MetricDiff {
    fn format_with_emoji(&self, emoji_threshold: Option<u32>) -> String {
        match self {
            MetricDiff::CallsCount(before, after) => {
                let diff_percent = calculate_percentage_diff(*before, *after);
                let emoji = get_emoji_for_diff(diff_percent, emoji_threshold);
                format!("{} → {} ({:+.1}%){}", before, after, diff_percent, emoji)
            }
            MetricDiff::DurationNs(before, after) => {
                let diff_percent = calculate_percentage_diff(*before, *after);
                let before_duration = Duration::from_nanos(*before);
                let after_duration = Duration::from_nanos(*after);
                let emoji = get_emoji_for_diff(diff_percent, emoji_threshold);
                format!(
                    "{:.2?} → {:.2?} ({:+.1}%){}",
                    before_duration, after_duration, diff_percent, emoji
                )
            }
            MetricDiff::Alloc(before, after) => {
                let diff_percent = calculate_percentage_diff(*before, *after);
                let emoji = get_emoji_for_diff(diff_percent, emoji_threshold);
                format!(
                    "{} → {} ({:+.1}%){}",
                    format_bytes(*before),
                    format_bytes(*after),
                    diff_percent,
                    emoji
                )
            }
            MetricDiff::Percentage(before, after) => {
                let diff_percent = calculate_percentage_diff(*before, *after);
                let before_percent = *before as f64 / 100.0;
                let after_percent = *after as f64 / 100.0;
                let emoji = get_emoji_for_diff(diff_percent, emoji_threshold);
                format!(
                    "{:.2}% → {:.2}% ({:+.1}%){}",
                    before_percent, after_percent, diff_percent, emoji
                )
            }
        }
    }
}

fn get_emoji_for_diff(diff_percent: f64, threshold: Option<u32>) -> &'static str {
    if let Some(threshold_val) = threshold {
        let threshold = threshold_val as f64;
        if diff_percent > threshold {
            " ⚠️ "
        } else if diff_percent < -threshold {
            " 🚀 "
        } else {
            "   "
        }
    } else {
        ""
    }
}

#[derive(Debug, Clone)]
pub struct MetricsComparison {
    pub total_elapsed_diff: MetricDiff,
    pub function_diffs: Vec<FunctionMetricsDiff>,
}

#[derive(Debug, Clone)]
pub struct FunctionMetricsDiff {
    pub function_name: String,
    pub metrics: Vec<MetricDiff>,
    pub is_removed: bool, // True if function was removed (no longer measured)
    pub is_new: bool,     // True if function is new (not in base)
}

fn calculate_percentage_diff(before: u64, after: u64) -> f64 {
    if before == 0 {
        if after == 0 {
            0.0
        } else {
            100.0 // 100% increase from 0
        }
    } else {
        ((after as f64 - before as f64) / before as f64) * 100.0
    }
}

fn find_function<'a>(data: &'a [JsonFunctionEntry], name: &str) -> Option<&'a JsonFunctionEntry> {
    data.iter().find(|f| f.name == name)
}

fn build_metrics_from_function(
    func: &JsonFunctionEntry,
    percentiles: &[u8],
    is_alloc: bool,
) -> Vec<(MetricKind, u64)> {
    let mut metrics = Vec::new();

    metrics.push((MetricKind::Calls, func.calls));

    if let Some(avg) = func.avg_raw {
        metrics.push((
            if is_alloc {
                MetricKind::Alloc
            } else {
                MetricKind::Duration
            },
            avg,
        ));
    }

    for p in percentiles {
        let key = format!("p{}", p);
        if let Some(&val) = func.percentiles_raw.get(&key) {
            metrics.push((
                if is_alloc {
                    MetricKind::Alloc
                } else {
                    MetricKind::Duration
                },
                val,
            ));
        }
    }

    if let Some(total) = func.total_raw {
        metrics.push((
            if is_alloc {
                MetricKind::Alloc
            } else {
                MetricKind::Duration
            },
            total,
        ));
    }

    if let Some(percent) = func.percent_total_raw {
        metrics.push((MetricKind::Percentage, percent));
    }

    metrics
}

#[derive(Debug, Clone, Copy)]
enum MetricKind {
    Calls,
    Duration,
    Alloc,
    Percentage,
}

fn compare_metrics(
    before_metrics: &JsonFunctionsList,
    after_metrics: &JsonFunctionsList,
) -> MetricsComparison {
    use hotpath::ProfilingMode;

    let is_alloc = matches!(before_metrics.hotpath_profiling_mode, ProfilingMode::Alloc);

    let total_elapsed_diff = MetricDiff::DurationNs(
        before_metrics.total_elapsed_raw,
        after_metrics.total_elapsed_raw,
    );

    let mut function_diffs = Vec::new();
    let mut new_functions = Vec::new();

    for after_func in &after_metrics.data {
        if let Some(before_func) = find_function(&before_metrics.data, &after_func.name) {
            let before_vals =
                build_metrics_from_function(before_func, &before_metrics.percentiles, is_alloc);
            let after_vals =
                build_metrics_from_function(after_func, &after_metrics.percentiles, is_alloc);

            let mut metrics = Vec::new();
            for ((kind, before_val), (_, after_val)) in before_vals.iter().zip(after_vals.iter()) {
                let diff = match kind {
                    MetricKind::Calls => MetricDiff::CallsCount(*before_val, *after_val),
                    MetricKind::Duration => MetricDiff::DurationNs(*before_val, *after_val),
                    MetricKind::Alloc => MetricDiff::Alloc(*before_val, *after_val),
                    MetricKind::Percentage => MetricDiff::Percentage(*before_val, *after_val),
                };
                metrics.push(diff);
            }

            function_diffs.push(FunctionMetricsDiff {
                function_name: after_func.name.clone(),
                metrics,
                is_removed: false,
                is_new: false,
            });
        } else {
            let after_vals =
                build_metrics_from_function(after_func, &after_metrics.percentiles, is_alloc);

            let metrics = after_vals
                .iter()
                .map(|(kind, after_val)| match kind {
                    MetricKind::Calls => MetricDiff::CallsCount(0, *after_val),
                    MetricKind::Duration => MetricDiff::DurationNs(0, *after_val),
                    MetricKind::Alloc => MetricDiff::Alloc(0, *after_val),
                    MetricKind::Percentage => MetricDiff::Percentage(0, *after_val),
                })
                .collect();

            new_functions.push(FunctionMetricsDiff {
                function_name: after_func.name.clone(),
                metrics,
                is_removed: false,
                is_new: true,
            });
        }
    }

    for before_func in &before_metrics.data {
        if find_function(&after_metrics.data, &before_func.name).is_none() {
            let before_vals =
                build_metrics_from_function(before_func, &before_metrics.percentiles, is_alloc);

            let metrics = before_vals
                .iter()
                .map(|(kind, before_val)| match kind {
                    MetricKind::Calls => MetricDiff::CallsCount(*before_val, 0),
                    MetricKind::Duration => MetricDiff::DurationNs(*before_val, 0),
                    MetricKind::Alloc => MetricDiff::Alloc(*before_val, 0),
                    MetricKind::Percentage => MetricDiff::Percentage(*before_val, 0),
                })
                .collect();

            function_diffs.push(FunctionMetricsDiff {
                function_name: before_func.name.clone(),
                metrics,
                is_removed: true,
                is_new: false,
            });
        }
    }

    function_diffs.extend(new_functions);

    function_diffs.sort_by(|a, b| {
        let a_percent = a
            .metrics
            .iter()
            .find_map(|m| {
                if let MetricDiff::Percentage(_, after) = m {
                    Some(*after)
                } else {
                    None
                }
            })
            .unwrap_or(0);

        let b_percent = b
            .metrics
            .iter()
            .find_map(|m| {
                if let MetricDiff::Percentage(_, after) = m {
                    Some(*after)
                } else {
                    None
                }
            })
            .unwrap_or(0);

        b_percent.cmp(&a_percent)
    });

    MetricsComparison {
        total_elapsed_diff,
        function_diffs,
    }
}

fn format_comparison_markdown(
    comparison: &MetricsComparison,
    metrics: &JsonFunctionsList,
    emoji_threshold: Option<u32>,
    benchmark_id: Option<&str>,
) -> String {
    let mut markdown = String::new();

    let base_branch = env::var("GITHUB_BASE_REF")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "base".to_string());
    let head_branch = env::var("GITHUB_HEAD_REF")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "head".to_string());

    markdown.push_str(&format!(
        "### Performance Comparison `{}` → `{}`\n",
        base_branch, head_branch
    ));
    markdown.push_str(&format!(
        "**Total Elapsed Time:** {}\n",
        comparison
            .total_elapsed_diff
            .format_with_emoji(emoji_threshold)
    ));
    markdown.push_str(&format!(
        "**Profiling Mode:** {} - {}\n",
        metrics.hotpath_profiling_mode, metrics.description
    ));
    if let Some(id) = benchmark_id {
        markdown.push_str(&format!("**Benchmark ID:** {}\n", id));
    }

    if comparison.function_diffs.is_empty() {
        markdown.push_str("*No functions to compare*\n");
        return markdown;
    }

    let mut table = Table::new();

    let mut header_cells = vec![Cell::new("Function"), Cell::new("Calls"), Cell::new("Avg")];
    for &p in &metrics.percentiles {
        header_cells.push(Cell::new(&format!("P{}", p)));
    }
    header_cells.push(Cell::new("Total"));
    header_cells.push(Cell::new("% Total"));
    table.add_row(Row::new(header_cells));

    for func_diff in &comparison.function_diffs {
        let function_display = if func_diff.is_removed {
            format!("️🗑️ {}", func_diff.function_name)
        } else if func_diff.is_new {
            format!("🆕 {}", func_diff.function_name)
        } else {
            func_diff.function_name.clone()
        };

        let mut row_cells = vec![Cell::new(&function_display)];
        for metric_diff in &func_diff.metrics {
            row_cells.push(Cell::new(&metric_diff.format_with_emoji(emoji_threshold)));
        }
        table.add_row(Row::new(row_cells));
    }

    markdown.push_str("```\n");
    markdown.push_str(&table.to_string());
    markdown.push_str("```\n\n");

    markdown.push_str("---\n");
    markdown.push_str("*Generated with [hotpath-rs](https://hotpath.rs)*\n");

    markdown
}

#[cfg(test)]
mod test {
    use super::*;
    use std::collections::HashMap;

    fn make_function_data(
        name: &str,
        calls: u64,
        avg: u64,
        p95: u64,
        total: u64,
        percent: u64,
    ) -> JsonFunctionEntry {
        let mut percentiles = HashMap::new();
        percentiles.insert("p95".to_string(), "formatted".to_string());
        let mut percentiles_raw = HashMap::new();
        percentiles_raw.insert("p95".to_string(), p95);

        JsonFunctionEntry {
            name: name.to_string(),
            calls,
            avg: "formatted".to_string(),
            avg_raw: Some(avg),
            percentiles,
            percentiles_raw,
            total: "formatted".to_string(),
            total_raw: Some(total),
            percent_total: "formatted".to_string(),
            percent_total_raw: Some(percent),
        }
    }

    fn make_metrics(data: Vec<JsonFunctionEntry>, total_elapsed_raw: u64) -> JsonFunctionsList {
        JsonFunctionsList {
            hotpath_profiling_mode: hotpath::ProfilingMode::Timing,
            time_elapsed: "formatted".to_string(),
            total_elapsed_ns: total_elapsed_raw,
            total_elapsed_raw,
            total_allocated: None,
            total_allocated_raw: None,
            description: "Time metrics".to_string(),
            caller_name: "test::main".to_string(),
            percentiles: vec![95],
            data,
        }
    }

    #[test]
    fn test_format_comparison_markdown() {
        let pr_data = vec![
            make_function_data(
                "basic::async_function",
                100,
                1256314,
                1276927,
                125631441,
                8940,
            ),
            make_function_data("basic::sync_function", 100, 61184, 62847, 6118443, 435),
            make_function_data("custom_block", 100, 62036, 64031, 6203646, 441),
        ];
        let pr_metrics = make_metrics(pr_data, 140515884);

        let main_data = vec![
            make_function_data(
                "basic::async_function",
                90,
                1130683,
                1149234,
                113068297,
                8046,
            ),
            make_function_data("basic::sync_function", 90, 55066, 56562, 5506599, 392),
            make_function_data("custom_block", 90, 55832, 57628, 5583281, 397),
        ];
        let main_metrics = make_metrics(main_data, 126464296);

        let comparison = compare_metrics(&main_metrics, &pr_metrics);

        println!("Total elapsed time diff: {}", comparison.total_elapsed_diff);

        for function_diff in &comparison.function_diffs {
            println!("Function: {}", function_diff.function_name);
            for metric_diff in &function_diff.metrics {
                println!("  {}", metric_diff);
            }
        }

        let markdown = format_comparison_markdown(&comparison, &main_metrics, Some(20), None);
        println!("\n=== Generated Markdown ===\n{}", markdown);
    }

    #[test]
    fn test_removed_function() {
        let pr_data = vec![make_function_data(
            "test::function_a",
            100,
            1000000,
            1100000,
            100000000,
            10000,
        )];
        let pr_metrics = make_metrics(pr_data, 100000000);

        let main_data = vec![
            make_function_data("test::function_a", 90, 900000, 1000000, 81000000, 9000),
            make_function_data("test::function_b", 50, 500000, 550000, 25000000, 2500),
        ];
        let main_metrics = make_metrics(main_data, 120000000);

        let comparison = compare_metrics(&main_metrics, &pr_metrics);

        println!("\n=== Test Removed Function ===");
        println!("Total elapsed time diff: {}", comparison.total_elapsed_diff);

        for function_diff in &comparison.function_diffs {
            println!(
                "Function: {} (removed: {})",
                function_diff.function_name, function_diff.is_removed
            );
            for metric_diff in &function_diff.metrics {
                println!("  {}", metric_diff);
            }
        }

        let markdown = format_comparison_markdown(&comparison, &main_metrics, Some(20), None);
        println!("\n=== Generated Markdown ===\n{}", markdown);

        assert!(comparison
            .function_diffs
            .iter()
            .any(|f| f.function_name == "test::function_b" && f.is_removed));
    }

    #[test]
    fn test_new_function() {
        let pr_data = vec![
            make_function_data("test::function_a", 100, 1000000, 1100000, 100000000, 8000),
            make_function_data("test::function_c", 60, 600000, 650000, 36000000, 2400),
        ];
        let pr_metrics = make_metrics(pr_data, 150000000);

        let main_data = vec![make_function_data(
            "test::function_a",
            90,
            900000,
            1000000,
            81000000,
            9000,
        )];
        let main_metrics = make_metrics(main_data, 120000000);

        let comparison = compare_metrics(&main_metrics, &pr_metrics);

        println!("\n=== Test New Function ===");
        println!("Total elapsed time diff: {}", comparison.total_elapsed_diff);

        for function_diff in &comparison.function_diffs {
            println!(
                "Function: {} (new: {}, removed: {})",
                function_diff.function_name, function_diff.is_new, function_diff.is_removed
            );
            for metric_diff in &function_diff.metrics {
                println!("  {}", metric_diff);
            }
        }

        let markdown = format_comparison_markdown(&comparison, &main_metrics, Some(20), None);
        println!("\n=== Generated Markdown ===\n{}", markdown);

        assert!(comparison
            .function_diffs
            .iter()
            .any(|f| f.function_name == "test::function_c" && f.is_new));
    }

    #[test]
    fn test_new_and_removed_functions() {
        let pr_data = vec![
            make_function_data("test::function_a", 100, 1000000, 1100000, 100000000, 7000),
            make_function_data("test::function_c", 40, 400000, 450000, 16000000, 1500),
        ];
        let pr_metrics = make_metrics(pr_data, 140000000);

        let main_data = vec![
            make_function_data("test::function_a", 90, 900000, 1000000, 81000000, 8000),
            make_function_data("test::function_b", 30, 300000, 350000, 9000000, 1200),
        ];
        let main_metrics = make_metrics(main_data, 120000000);

        let comparison = compare_metrics(&main_metrics, &pr_metrics);

        println!("\n=== Test New and Removed Functions ===");
        println!("Total elapsed time diff: {}", comparison.total_elapsed_diff);

        for function_diff in &comparison.function_diffs {
            println!(
                "Function: {} (new: {}, removed: {})",
                function_diff.function_name, function_diff.is_new, function_diff.is_removed
            );
        }

        let markdown = format_comparison_markdown(&comparison, &main_metrics, Some(20), None);
        println!("\n=== Generated Markdown ===\n{}", markdown);

        assert_eq!(comparison.function_diffs.len(), 3);
        assert!(comparison
            .function_diffs
            .iter()
            .any(|f| f.function_name == "test::function_b" && f.is_removed));
        assert!(comparison
            .function_diffs
            .iter()
            .any(|f| f.function_name == "test::function_c" && f.is_new));
        assert!(comparison
            .function_diffs
            .iter()
            .any(|f| f.function_name == "test::function_a" && !f.is_new && !f.is_removed));
    }
}
