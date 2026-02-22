use clap::Parser;
use eyre::Result;
use hotpath::json::JsonReport;
use prettytable::{Cell, Row, Table};
use std::fs;

use hotpath::shorten_function_name;

use crate::cmd::shared::{compare_reports, JsonReportDiff, MetricsComparison};

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

        let diff = compare_reports(&before, &after);
        print_diff(&diff);

        Ok(())
    }
}

fn print_diff(diff: &JsonReportDiff) {
    if diff.functions_timing.is_none() && diff.functions_alloc.is_none() {
        println!("No comparable sections found in both reports.");
        return;
    }

    if diff.before_label.is_some() || diff.after_label.is_some() {
        let before = diff.before_label.as_deref().unwrap_or("(unlabeled)");
        let after = diff.after_label.as_deref().unwrap_or("(unlabeled)");
        println!("Comparing: {} → {}", before, after);
    }

    println!("Total Elapsed: {}", diff.total_elapsed_diff);
    if let Some(cpu_baseline) = &diff.cpu_baseline_diff {
        println!("CPU Baseline: {}", cpu_baseline);
    }
    println!();

    if let Some(comparison) = &diff.functions_timing {
        println!(
            "Functions Timing ({} - {})",
            comparison.profiling_mode, comparison.description
        );
        print_comparison_table(comparison);
        println!();
    }

    if let Some(comparison) = &diff.functions_alloc {
        println!(
            "Functions Alloc ({} - {})",
            comparison.profiling_mode, comparison.description
        );
        print_comparison_table(comparison);
        println!();
    }
}

fn print_comparison_table(comparison: &MetricsComparison) {
    if comparison.function_diffs.is_empty() {
        println!("No functions to compare.");
        return;
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
        let name = if func_diff.is_removed {
            format!("[removed] {}", short_name)
        } else if func_diff.is_new {
            format!("[new] {}", short_name)
        } else {
            short_name
        };

        let mut row_cells = vec![Cell::new(&name)];
        for metric in &func_diff.metrics {
            row_cells.push(Cell::new(&format!("{}", metric)));
        }
        table.add_row(Row::new(row_cells));
    }

    table.printstd();
}
