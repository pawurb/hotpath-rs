mod comment;

use crate::cmd::shared::{
    build_functions_table, build_threads_table, compare_reports, format_threads_globals,
    JsonReportDiff,
};
use clap::Parser;
use comment::upsert_pr_comment;
use eyre::Result;
use hotpath::json::JsonReport;
use std::env;
use std::path::PathBuf;

#[derive(Debug, Parser)]
pub struct ProfilePrArgs {
    #[arg(long, help = "Path to JSON metrics file from head branch")]
    head_metrics: PathBuf,

    #[arg(long, help = "Path to JSON metrics file from base branch")]
    base_metrics: PathBuf,

    #[arg(long, help = "GitHub token for API access")]
    github_token: Option<String>,

    #[arg(long, help = "Pull request number")]
    pr_number: Option<String>,

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

    #[arg(long, help = "Print the generated comment to stdout without posting")]
    dry_run: bool,
}

fn read_report(path: &PathBuf) -> Result<JsonReport> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| eyre::eyre!("Failed to read {}: {}", path.display(), e))?;

    serde_json::from_str(&content)
        .map_err(|e| eyre::eyre!("Failed to parse {}: {}", path.display(), e))
}

impl ProfilePrArgs {
    pub fn run(&self) -> Result<()> {
        let emoji_threshold = if let Some(0) = self.emoji_threshold {
            None
        } else {
            Some(self.emoji_threshold.unwrap_or(20))
        };

        let base_report = read_report(&self.base_metrics)?;
        let head_report = read_report(&self.head_metrics)?;

        let diff = compare_reports(&base_report, &head_report);

        if diff.functions_timing.is_none()
            && diff.functions_alloc.is_none()
            && diff.threads.is_none()
        {
            println!("No comparable sections found between head and base reports");
            return Ok(());
        }

        let body = format_diff_markdown(&diff, emoji_threshold, self.benchmark_id.as_deref());

        if self.dry_run {
            print!("{}", body);
            return Ok(());
        }

        let github_token = self
            .github_token
            .as_deref()
            .ok_or_else(|| eyre::eyre!("--github-token is required when not using --dry-run"))?;
        let pr_number = self
            .pr_number
            .as_deref()
            .ok_or_else(|| eyre::eyre!("--pr-number is required when not using --dry-run"))?;
        let repo = env::var("GITHUB_REPOSITORY").unwrap_or_default();

        if repo.is_empty() || pr_number.is_empty() {
            println!("No PR context found, skipping comment posting");
            return Ok(());
        }

        let profiling_mode = diff
            .functions_timing
            .as_ref()
            .or(diff.functions_alloc.as_ref())
            .map(|c| c.profiling_mode.clone())
            .unwrap();

        match upsert_pr_comment(
            &repo,
            pr_number,
            github_token,
            &body,
            &profiling_mode,
            self.benchmark_id.as_deref(),
        ) {
            Ok(_) => {}
            Err(e) => println!("Failed to post/update comment: {}", e),
        }

        Ok(())
    }
}

fn format_diff_markdown(
    diff: &JsonReportDiff,
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
        diff.total_elapsed_diff.format_with_emoji(emoji_threshold)
    ));
    if let Some(cpu_baseline) = &diff.cpu_baseline_diff {
        markdown.push_str(&format!(
            "**CPU Baseline:** {}\n",
            cpu_baseline.format_with_emoji(emoji_threshold)
        ));
    }
    if let Some(id) = benchmark_id {
        markdown.push_str(&format!("**Benchmark ID:** {}\n", id));
    }

    if let Some(comparison) = &diff.functions_timing {
        markdown.push_str(&format!(
            "\n#### Timing ({} - {})\n",
            comparison.profiling_mode, comparison.description
        ));
        if comparison.function_diffs.is_empty() {
            markdown.push_str("*No functions to compare*\n");
        } else {
            markdown.push_str(&format!(
                "```\n{}```\n",
                build_functions_table(comparison, emoji_threshold)
            ));
        }
    }

    if let Some(comparison) = &diff.functions_alloc {
        markdown.push_str(&format!(
            "\n#### Allocations ({} - {})\n",
            comparison.profiling_mode, comparison.description
        ));
        if comparison.function_diffs.is_empty() {
            markdown.push_str("*No functions to compare*\n");
        } else {
            markdown.push_str(&format!(
                "```\n{}```\n",
                build_functions_table(comparison, emoji_threshold)
            ));
        }
    }

    if let Some(threads) = &diff.threads {
        markdown.push_str("\n#### Threads\n");
        if threads.thread_diffs.is_empty() {
            markdown.push_str("*No threads to compare*\n");
        } else {
            if let Some(globals) = format_threads_globals(threads, emoji_threshold) {
                markdown.push_str(&format!("**{}**\n", globals));
            }
            markdown.push_str(&format!(
                "```\n{}```\n",
                build_threads_table(threads, emoji_threshold)
            ));
        }
    }

    markdown.push_str("\n---\n");
    markdown.push_str("*Generated with [hotpath-rs](https://hotpath.rs)*\n");

    markdown
}
