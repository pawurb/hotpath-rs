use clap::Parser;
use eyre::Result;
use hotpath::json::JsonReport;
use std::env;
use std::fs;
use std::io::IsTerminal;

use crate::cmd::shared::{
    build_functions_table, build_threads_table, compare_reports, format_threads_globals,
    JsonReportDiff,
};

fn use_colors() -> bool {
    env::var("NO_COLOR").is_err()
}

fn print_table(table: &hotpath::table::Table) {
    let stdout = std::io::stdout();
    let colors = use_colors() && stdout.is_terminal();
    let _ = table.print(&mut stdout.lock(), colors);
}

#[derive(Debug, Parser)]
pub struct CompareArgs {
    #[arg(long, help = "Path to the before JSON report")]
    before_json_path: String,

    #[arg(long, help = "Path to the after JSON report")]
    after_json_path: String,
}

impl CompareArgs {
    pub fn run(&self) -> Result<()> {
        let before_raw = fs::read_to_string(&self.before_json_path)
            .map_err(|e| eyre::eyre!("Failed to read before JSON: {}", e))?;
        let after_raw = fs::read_to_string(&self.after_json_path)
            .map_err(|e| eyre::eyre!("Failed to read after JSON: {}", e))?;

        let before: JsonReport = serde_json::from_str(&before_raw)
            .map_err(|e| eyre::eyre!("Failed to parse before JSON: {}", e))?;
        let after: JsonReport = serde_json::from_str(&after_raw)
            .map_err(|e| eyre::eyre!("Failed to parse after JSON: {}", e))?;

        let diff = compare_reports(&before, &after).map_err(|e| eyre::eyre!("{}", e))?;
        print_diff(&diff);

        Ok(())
    }
}

fn print_diff(diff: &JsonReportDiff) {
    if diff.functions_timing.is_none() && diff.functions_alloc.is_none() && diff.threads.is_none() {
        println!("No comparable sections found.");
        return;
    }

    if diff.before_label.is_some() || diff.after_label.is_some() {
        let before = diff.before_label.as_deref().unwrap_or("(unlabeled)");
        let after = diff.after_label.as_deref().unwrap_or("(unlabeled)");
        println!("Comparing: {} → {}", before, after);
    }

    println!("Total Elapsed: {}", diff.total_elapsed_diff);
    println!();

    if let Some(comparison) = &diff.functions_timing {
        println!("{} - {}", comparison.profiling_mode, comparison.description);
        if comparison.function_diffs.is_empty() {
            println!("No functions to compare.");
        } else {
            print_table(&build_functions_table(comparison, None));
        }
        println!();
    }

    if let Some(comparison) = &diff.functions_alloc {
        println!("{} - {}", comparison.profiling_mode, comparison.description);
        if comparison.function_diffs.is_empty() {
            println!("No functions to compare.");
        } else {
            print_table(&build_functions_table(comparison, None));
        }
        println!();
    }

    if let Some(threads) = &diff.threads {
        println!("Threads");
        if threads.thread_diffs.is_empty() {
            println!("No threads to compare.");
        } else {
            let globals = format_threads_globals(threads, None);
            if !globals.is_empty() {
                println!("{}", globals.join(" | "));
            }
            print_table(&build_threads_table(threads, None));
        }
        println!();
    }
}
