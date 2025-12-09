use crate::output::{
    shorten_function_name, FunctionsDataJson, FunctionsJson, MetricType, MetricsProvider, Reporter,
};
use colored::*;
use prettytable::{color, Attr, Cell, Row, Table};
use std::collections::HashMap;
use std::time::Duration;

pub(crate) fn get_sorted_measurements(
    metrics_provider: &dyn MetricsProvider<'_>,
) -> Vec<(String, Vec<MetricType>)> {
    let metric_data = metrics_provider.metric_data();

    let mut sorted_entries: Vec<(String, Vec<MetricType>)> = metric_data.into_iter().collect();
    sorted_entries.sort_by(|(name_a, metrics_a), (name_b, metrics_b)| {
        let key_a = metrics_provider.sort_key(metrics_a);
        let key_b = metrics_provider.sort_key(metrics_b);

        key_b
            .partial_cmp(&key_a)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| name_a.cmp(name_b))
    });

    sorted_entries
}

pub(crate) fn display_table(metrics_provider: &dyn MetricsProvider<'_>) {
    let use_colors = std::env::var("NO_COLOR").is_err();

    let mut table = Table::new();

    let header_cells: Vec<Cell> = metrics_provider
        .headers()
        .into_iter()
        .map(|header| {
            if use_colors {
                Cell::new(&header)
                    .with_style(Attr::Bold)
                    .with_style(Attr::ForegroundColor(color::CYAN))
            } else {
                Cell::new(&header).with_style(Attr::Bold)
            }
        })
        .collect();

    table.add_row(Row::new(header_cells));

    let sorted_entries = get_sorted_measurements(metrics_provider);

    for (function_name, metrics) in sorted_entries {
        let mut row_cells = Vec::new();

        let short_name = shorten_function_name(&function_name);
        row_cells.push(Cell::new(&short_name));

        for metric in &metrics {
            row_cells.push(Cell::new(&metric.to_string()));
        }

        table.add_row(Row::new(row_cells));
    }

    println!(
        "{} {} - {}",
        "[hotpath]".blue().bold(),
        metrics_provider.profiling_mode(),
        metrics_provider.description()
    );

    let (displayed, total) = metrics_provider.entry_counts();
    if displayed < total {
        println!(
            "{}: {:.2?} ({}/{})",
            metrics_provider.caller_name().yellow().bold(),
            Duration::from_nanos(metrics_provider.total_elapsed()),
            displayed,
            total
        );
    } else {
        println!(
            "{}: {:.2?}",
            metrics_provider.caller_name().yellow().bold(),
            Duration::from_nanos(metrics_provider.total_elapsed()),
        );
    }

    table.printstd();

    if metrics_provider.has_unsupported_async() {
        println!();
        println!(
            "* {} for async methods is currently only available for tokio {} runtime.",
            "alloc profiling".yellow().bold(),
            "current_thread".green().bold()
        );
        println!(
            "  Please use {} to enable it.",
            "#[tokio::main(flavor = \"current_thread\")]".cyan().bold()
        );
    }
}

fn display_no_measurements_message(total_elapsed: Duration, caller_name: &str) {
    let title = format!(
        "\n{} No measurements recorded from {} (Total time: {:.2?})",
        "[hotpath]".blue().bold(),
        caller_name.yellow().bold(),
        total_elapsed
    );
    println!("{title}");
    println!();
    println!(
        "To start measuring performance, add the {} macro to your functions:",
        "#[hotpath::measure]".cyan().bold()
    );
    println!();
    println!(
        "  {}",
        "#[cfg_attr(feature = \"hotpath\", hotpath::measure)]".cyan()
    );
    println!("  {}", "fn your_function() {".dimmed());
    println!("  {}", "    // your code here".dimmed());
    println!("  {}", "}".dimmed());
    println!();
    println!(
        "Or use {} to measure code blocks:",
        "hotpath::measure_block!".cyan().bold()
    );
    println!();
    println!("  {}", "#[cfg(feature = \"hotpath\")]".cyan());
    println!("  {}", "hotpath::measure_block!(\"label\", {".cyan());
    println!("  {}", "    // your code here".dimmed());
    println!("  {}", "});".cyan());
    println!();
}

pub(crate) struct TableReporter;

impl Reporter for TableReporter {
    fn report(
        &self,
        metrics_provider: &dyn MetricsProvider<'_>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if metrics_provider.metric_data().is_empty() {
            display_no_measurements_message(
                Duration::from_nanos(metrics_provider.total_elapsed()),
                metrics_provider.caller_name(),
            );
            return Ok(());
        }

        display_table(metrics_provider);
        Ok(())
    }
}

pub(crate) struct JsonReporter;

impl Reporter for JsonReporter {
    fn report(
        &self,
        metrics_provider: &dyn MetricsProvider<'_>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if metrics_provider.metric_data().is_empty() {
            display_no_measurements_message(Duration::ZERO, metrics_provider.caller_name());
            return Ok(());
        }

        let json = FunctionsJson::from(metrics_provider);
        println!("{}", serde_json::to_string(&json).unwrap());
        Ok(())
    }
}

pub(crate) struct JsonPrettyReporter;

impl Reporter for JsonPrettyReporter {
    fn report(
        &self,
        metrics_provider: &dyn MetricsProvider<'_>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if metrics_provider.metric_data().is_empty() {
            display_no_measurements_message(Duration::ZERO, metrics_provider.caller_name());
            return Ok(());
        }

        let json = FunctionsJson::from(metrics_provider);
        println!("{}", serde_json::to_string_pretty(&json)?);
        Ok(())
    }
}

impl From<&dyn MetricsProvider<'_>> for FunctionsJson {
    fn from(metrics: &dyn MetricsProvider<'_>) -> Self {
        let hotpath_profiling_mode = metrics.profiling_mode();
        let percentiles = metrics.percentiles();

        let sorted_entries = get_sorted_measurements(metrics);
        let data: HashMap<String, Vec<MetricType>> = sorted_entries.into_iter().collect();

        Self {
            hotpath_profiling_mode,
            total_elapsed: metrics.total_elapsed(),
            description: metrics.description(),
            caller_name: metrics.caller_name().to_string(),
            percentiles,
            data: FunctionsDataJson(data),
        }
    }
}
