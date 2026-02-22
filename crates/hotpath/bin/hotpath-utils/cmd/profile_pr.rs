mod comment;

use crate::cmd::shared::{compare_metrics, FunctionsComparison, MetricDiff};
use clap::Parser;
use comment::upsert_pr_comment;
use eyre::Result;
use hotpath::json::JsonFunctionsList;
use hotpath::shorten_function_name;
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
        let total_elapsed_diff = MetricDiff::DurationNs(
            base_metrics_data.total_elapsed_ns,
            head_metrics_data.total_elapsed_ns,
        );
        let comparison_markdown = format_comparison_markdown(
            &comparison,
            &total_elapsed_diff,
            None,
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

fn format_comparison_markdown(
    comparison: &FunctionsComparison,
    total_elapsed_diff: &MetricDiff,
    cpu_baseline_diff: Option<&MetricDiff>,
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
        total_elapsed_diff.format_with_emoji(emoji_threshold)
    ));
    if let Some(cpu_baseline) = cpu_baseline_diff {
        markdown.push_str(&format!(
            "**CPU Baseline:** {}\n",
            cpu_baseline.format_with_emoji(emoji_threshold)
        ));
    }
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
        let short_name = shorten_function_name(&func_diff.function_name);
        let function_display = if func_diff.is_removed {
            format!("️🗑️ {}", short_name)
        } else if func_diff.is_new {
            format!("🆕 {}", short_name)
        } else {
            short_name
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
