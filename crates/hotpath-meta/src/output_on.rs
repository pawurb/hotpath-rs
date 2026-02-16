use crate::output::{shorten_function_name, MetricType, MetricsProvider};
use colored::*;
use prettytable::{color, Attr, Cell, Row, Table};
use std::io::Write;
use std::time::Duration;

pub(crate) fn get_sorted_measurements(
    metrics_provider: &dyn MetricsProvider<'_>,
) -> Vec<(&'static str, Vec<MetricType>)> {
    let metric_data = metrics_provider.metric_data();

    let mut sorted_entries: Vec<(&'static str, Vec<MetricType>)> =
        metric_data.into_iter().collect();
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

pub(crate) fn display_table_to<W: Write>(
    writer: &mut W,
    metrics_provider: &dyn MetricsProvider<'_>,
    use_colors: bool,
) {
    let use_colors = use_colors && std::env::var("NO_COLOR").is_err();

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

        let short_name = shorten_function_name(function_name);
        row_cells.push(Cell::new(&short_name));

        for metric in &metrics {
            row_cells.push(Cell::new(&metric.to_string()));
        }

        table.add_row(Row::new(row_cells));
    }

    if use_colors {
        let _ = writeln!(
            writer,
            "{} {} - {}",
            "[hotpath]".blue().bold(),
            metrics_provider.profiling_mode(),
            metrics_provider.description()
        );
    } else {
        let _ = writeln!(
            writer,
            "[hotpath] {} - {}",
            metrics_provider.profiling_mode(),
            metrics_provider.description()
        );
    }

    let (displayed, total) = metrics_provider.entry_counts();
    if displayed < total {
        if use_colors {
            let _ = writeln!(
                writer,
                "{}: {:.2?} ({}/{})",
                metrics_provider.caller_name().yellow().bold(),
                Duration::from_nanos(metrics_provider.total_elapsed()),
                displayed,
                total
            );
        } else {
            let _ = writeln!(
                writer,
                "{}: {:.2?} ({}/{})",
                metrics_provider.caller_name(),
                Duration::from_nanos(metrics_provider.total_elapsed()),
                displayed,
                total
            );
        }
    } else if use_colors {
        let _ = writeln!(
            writer,
            "{}: {:.2?}",
            metrics_provider.caller_name().yellow().bold(),
            Duration::from_nanos(metrics_provider.total_elapsed()),
        );
    } else {
        let _ = writeln!(
            writer,
            "{}: {:.2?}",
            metrics_provider.caller_name(),
            Duration::from_nanos(metrics_provider.total_elapsed()),
        );
    }

    let _ = table.print(writer);

    if metrics_provider.has_unsupported_async() {
        let _ = writeln!(writer);
        if use_colors {
            let _ = writeln!(
                writer,
                "* {} for async methods is currently only available for tokio {} runtime.",
                "alloc profiling".yellow().bold(),
                "current_thread".green().bold()
            );
            let _ = writeln!(
                writer,
                "  Please use {} to enable it.",
                "#[tokio::main(flavor = \"current_thread\")]".cyan().bold()
            );
        } else {
            let _ = writeln!(
                writer,
                "* alloc profiling for async methods is currently only available for tokio current_thread runtime."
            );
            let _ = writeln!(
                writer,
                "  Please use #[tokio::main(flavor = \"current_thread\")] to enable it."
            );
        }
    }
}

pub(crate) fn display_no_measurements_message_to<W: Write>(
    writer: &mut W,
    total_elapsed: Duration,
    caller_name: &str,
    use_colors: bool,
) {
    let use_colors = use_colors && std::env::var("NO_COLOR").is_err();

    if use_colors {
        let _ = writeln!(
            writer,
            "\n{} No measurements recorded from {} (Total time: {:.2?})",
            "[hotpath]".blue().bold(),
            caller_name.yellow().bold(),
            total_elapsed
        );
        let _ = writeln!(writer);
        let _ = writeln!(
            writer,
            "To start measuring performance, add the {} macro to your functions:",
            "#[hotpath_meta::measure]".cyan().bold()
        );
        let _ = writeln!(writer);
        let _ = writeln!(
            writer,
            "  {}",
            "#[cfg_attr(feature = \"hotpath\", hotpath_meta::measure)]".cyan()
        );
        let _ = writeln!(writer, "  {}", "fn your_function() {".dimmed());
        let _ = writeln!(writer, "  {}", "    // your code here".dimmed());
        let _ = writeln!(writer, "  {}", "}".dimmed());
        let _ = writeln!(writer);
        let _ = writeln!(
            writer,
            "Or use {} to measure code blocks:",
            "hotpath_meta::measure_block!".cyan().bold()
        );
        let _ = writeln!(writer);
        let _ = writeln!(writer, "  {}", "#[cfg(feature = \"hotpath\")]".cyan());
        let _ = writeln!(
            writer,
            "  {}",
            "hotpath_meta::measure_block!(\"label\", {".cyan()
        );
        let _ = writeln!(writer, "  {}", "    // your code here".dimmed());
        let _ = writeln!(writer, "  {}", "});".cyan());
        let _ = writeln!(writer);
    } else {
        let _ = writeln!(
            writer,
            "\n[hotpath] No measurements recorded from {} (Total time: {:.2?})",
            caller_name, total_elapsed
        );
        let _ = writeln!(writer);
        let _ = writeln!(
            writer,
            "To start measuring performance, add the #[hotpath_meta::measure] macro to your functions:"
        );
        let _ = writeln!(writer);
        let _ = writeln!(
            writer,
            "  #[cfg_attr(feature = \"hotpath\", hotpath_meta::measure)]"
        );
        let _ = writeln!(writer, "  fn your_function() {{");
        let _ = writeln!(writer, "      // your code here");
        let _ = writeln!(writer, "  }}");
        let _ = writeln!(writer);
        let _ = writeln!(
            writer,
            "Or use hotpath_meta::measure_block! to measure code blocks:"
        );
        let _ = writeln!(writer);
        let _ = writeln!(writer, "  #[cfg(feature = \"hotpath\")]");
        let _ = writeln!(writer, "  hotpath_meta::measure_block!(\"label\", {{");
        let _ = writeln!(writer, "      // your code here");
        let _ = writeln!(writer, "  }});");
        let _ = writeln!(writer);
    }
}
