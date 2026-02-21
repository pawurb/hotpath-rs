mod comment;

use crate::cmd::shared::{compare_metrics, MetricsComparison};
use clap::Parser;
use comment::upsert_pr_comment;
use eyre::Result;
use hotpath::json::JsonFunctionsList;
use prettytable::{Cell, Row, Table};
use std::env;

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
        let comparison_markdown =
            format_comparison_markdown(&comparison, emoji_threshold, self.benchmark_id.as_deref());

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

fn format_comparison_markdown(
    comparison: &MetricsComparison,
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
        comparison.profiling_mode, comparison.description
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
    for &p in &comparison.percentiles {
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
    use crate::cmd::shared::{compare_metrics, compare_reports};
    use hotpath::json::{JsonFunctionEntry, JsonFunctionsList, JsonReport};
    use std::collections::HashMap;

    use super::*;

    fn make_function_data(
        name: &str,
        calls: u64,
        avg: u64,
        p95: u64,
        total: u64,
        percent: u64,
    ) -> JsonFunctionEntry {
        let mut percentiles = HashMap::new();
        percentiles.insert("p95".to_string(), hotpath::format_duration(p95));

        JsonFunctionEntry {
            id: 0,
            name: name.to_string(),
            calls,
            avg: hotpath::format_duration(avg),
            percentiles,
            total: hotpath::format_duration(total),
            percent_total: format!("{:.2}%", percent as f64 / 100.0),
        }
    }

    fn make_metrics(data: Vec<JsonFunctionEntry>, total_elapsed_ns: u64) -> JsonFunctionsList {
        JsonFunctionsList {
            hotpath_profiling_mode: hotpath::ProfilingMode::Timing,
            time_elapsed: hotpath::format_duration(total_elapsed_ns),
            total_elapsed_ns,
            total_allocated: None,
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

        let markdown = format_comparison_markdown(&comparison, Some(20), None);
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

        let markdown = format_comparison_markdown(&comparison, Some(20), None);
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

        let markdown = format_comparison_markdown(&comparison, Some(20), None);
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

        let markdown = format_comparison_markdown(&comparison, Some(20), None);
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

    fn make_alloc_function_data(
        name: &str,
        calls: u64,
        avg_bytes: u64,
        p95_bytes: u64,
        total_bytes: u64,
        percent: u64,
    ) -> JsonFunctionEntry {
        let mut percentiles = HashMap::new();
        percentiles.insert("p95".to_string(), hotpath::format_bytes(p95_bytes));

        JsonFunctionEntry {
            id: 0,
            name: name.to_string(),
            calls,
            avg: hotpath::format_bytes(avg_bytes),
            percentiles,
            total: hotpath::format_bytes(total_bytes),
            percent_total: format!("{:.2}%", percent as f64 / 100.0),
        }
    }

    fn make_alloc_metrics(
        data: Vec<JsonFunctionEntry>,
        total_elapsed_ns: u64,
    ) -> JsonFunctionsList {
        JsonFunctionsList {
            hotpath_profiling_mode: hotpath::ProfilingMode::Alloc,
            time_elapsed: hotpath::format_duration(total_elapsed_ns),
            total_elapsed_ns,
            total_allocated: Some("10.00 MB".to_string()),
            description: "Alloc metrics".to_string(),
            caller_name: "test::main".to_string(),
            percentiles: vec![95],
            data,
        }
    }

    fn make_report(
        timing: Option<JsonFunctionsList>,
        alloc: Option<JsonFunctionsList>,
    ) -> JsonReport {
        JsonReport {
            label: None,
            functions_timing: timing,
            functions_alloc: alloc,
            channels: None,
            streams: None,
            futures: None,
            threads: None,
            cpu_baseline: None,
        }
    }

    #[test]
    fn test_compare_reports_both_sections() {
        let before_timing = make_metrics(
            vec![make_function_data(
                "fn_a", 100, 1000000, 1100000, 100000000, 10000,
            )],
            100000000,
        );
        let after_timing = make_metrics(
            vec![make_function_data(
                "fn_a", 120, 1200000, 1300000, 144000000, 10000,
            )],
            144000000,
        );
        let before_alloc = make_alloc_metrics(
            vec![make_alloc_function_data(
                "fn_a", 100, 1024, 2048, 102400, 10000,
            )],
            100000000,
        );
        let after_alloc = make_alloc_metrics(
            vec![make_alloc_function_data(
                "fn_a", 120, 2048, 4096, 245760, 10000,
            )],
            144000000,
        );

        let before = make_report(Some(before_timing), Some(before_alloc));
        let after = make_report(Some(after_timing), Some(after_alloc));

        let diff = compare_reports(&before, &after);

        assert!(diff.functions_timing.is_some());
        assert!(diff.functions_alloc.is_some());

        let timing = diff.functions_timing.unwrap();
        assert_eq!(timing.function_diffs.len(), 1);
        assert_eq!(timing.function_diffs[0].function_name, "fn_a");

        let alloc = diff.functions_alloc.unwrap();
        assert_eq!(alloc.function_diffs.len(), 1);
        assert_eq!(alloc.function_diffs[0].function_name, "fn_a");
    }

    #[test]
    fn test_compare_reports_one_section_missing() {
        let before_timing = make_metrics(
            vec![make_function_data(
                "fn_a", 100, 1000000, 1100000, 100000000, 10000,
            )],
            100000000,
        );
        let after_timing = make_metrics(
            vec![make_function_data(
                "fn_a", 120, 1200000, 1300000, 144000000, 10000,
            )],
            144000000,
        );

        let before = make_report(Some(before_timing), None);
        let after = make_report(Some(after_timing), None);

        let diff = compare_reports(&before, &after);

        assert!(diff.functions_timing.is_some());
        assert!(diff.functions_alloc.is_none());
    }

    #[test]
    fn test_compare_reports_both_missing() {
        let before = make_report(None, None);
        let after = make_report(None, None);

        let diff = compare_reports(&before, &after);

        assert!(diff.functions_timing.is_none());
        assert!(diff.functions_alloc.is_none());
    }
}
