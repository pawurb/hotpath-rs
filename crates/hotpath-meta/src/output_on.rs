use crate::output::{format_duration, shorten_function_name, MetricType, MetricsProvider};
use crate::shared::Section;
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

pub(crate) fn write_report_header<W: Write + ?Sized>(
    writer: &mut W,
    elapsed: Duration,
    sections: &[Section],
    cpu_baseline_ns: Option<u64>,
    label: Option<&str>,
) {
    let section_names: Vec<&str> = sections.iter().map(|s| s.short_name()).collect();
    let sections_str = section_names.join(", ");
    let baseline_str = cpu_baseline_ns
        .map(|ns| format!(" (CPU baseline avg: {})", format_duration(ns)))
        .unwrap_or_default();
    let label_str = label.map(|l| format!(" | {}", l)).unwrap_or_default();

    let _ = writeln!(
        writer,
        "[hotpath-meta] {:.2?} | {}{}{}",
        elapsed, sections_str, baseline_str, label_str,
    );
    let _ = writeln!(writer);
}

pub(crate) fn write_section_header<W: Write + ?Sized>(
    writer: &mut W,
    section_name: &str,
    description: &str,
) {
    let _ = write!(writer, "{} - {}", section_name, description);
}

fn print_table<W: Write>(table: &Table, writer: &mut W) {
    if crate::output::use_colors() {
        let _ = table.print_tty(false);
    } else {
        let _ = table.print(writer);
    }
}

pub(crate) fn display_table_to<W: Write>(
    writer: &mut W,
    metrics_provider: &dyn MetricsProvider<'_>,
) {
    let use_colors = crate::output::use_colors();
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

    let mode = metrics_provider.profiling_mode().to_string();
    let desc = metrics_provider.description();
    let (displayed, total) = metrics_provider.entry_counts();
    if displayed < total {
        let _ = writeln!(writer, "{} - {} ({}/{})", mode, desc, displayed, total);
    } else {
        write_section_header(writer, &mode, &desc);
        let _ = writeln!(writer);
    }

    print_table(&table, writer);

    if metrics_provider.has_unsupported_async() {
        let _ = writeln!(
            writer,
            "* alloc profiling for async methods is currently only available for tokio current_thread runtime."
        );
        let _ = writeln!(
            writer,
            "  Please use #[tokio::main(flavor = \"current_thread\")] to enable it."
        );
    }

    let _ = writeln!(writer);
}

pub(crate) fn display_no_measurements_message_to<W: Write>(
    writer: &mut W,
    total_elapsed: Duration,
    caller_name: &str,
) {
    let _ = writeln!(
        writer,
        "\n[hotpath-meta] No measurements recorded from {} (Total time: {:.2?})",
        caller_name, total_elapsed
    );
    let _ = writeln!(writer);
    let _ = writeln!(
        writer,
        "To start measuring performance, add the #[hotpath_meta::measure] macro to your functions:"
    );
    let _ = writeln!(writer);
    let _ = writeln!(writer, "  #[hotpath_meta::measure]");
    let _ = writeln!(writer, "  fn your_function() {{");
    let _ = writeln!(writer, "      // your code here");
    let _ = writeln!(writer, "  }}");
    let _ = writeln!(writer);
}
