use clap::Parser;
use eyre::Result;
use hotpath::json::JsonReport;
use prettytable::{Cell, Row, Table};
use std::fs;

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

    if let Some(comparison) = &diff.functions_timing {
        println!(
            "Functions Timing ({} - {})",
            comparison.profiling_mode, comparison.description
        );
        println!("Total Elapsed: {}", comparison.total_elapsed_diff);
        print_comparison_table(comparison);
        println!(
            "Before: {} | After: {}",
            comparison.before_elapsed, comparison.after_elapsed
        );
        println!();
    }

    if let Some(comparison) = &diff.functions_alloc {
        println!(
            "Functions Alloc ({} - {})",
            comparison.profiling_mode, comparison.description
        );
        println!("Total Elapsed: {}", comparison.total_elapsed_diff);
        print_comparison_table(comparison);
        println!(
            "Before: {} | After: {}",
            comparison.before_elapsed, comparison.after_elapsed
        );
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
        let name = if func_diff.is_removed {
            format!("[removed] {}", func_diff.function_name)
        } else if func_diff.is_new {
            format!("[new] {}", func_diff.function_name)
        } else {
            func_diff.function_name.clone()
        };

        let mut row_cells = vec![Cell::new(&name)];
        for metric in &func_diff.metrics {
            row_cells.push(Cell::new(&format!("{}", metric)));
        }
        table.add_row(Row::new(row_cells));
    }

    table.printstd();
}
