use crate::json::JsonFunctionsList;
use crate::output::{
    format_duration, format_percentile_header, format_percentile_key, shorten_function_name,
};
use crate::shared::Section;
use prettytable::{color, Attr, Cell, Row, Table};
use std::io::Write;
use std::time::Duration;

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

pub(crate) fn display_functions_table_to<W: Write>(writer: &mut W, list: &JsonFunctionsList) {
    let use_colors = crate::output::use_colors();
    let mut table = Table::new();

    let mut header_names = vec![
        "Function".to_string(),
        "Calls".to_string(),
        "Avg".to_string(),
    ];
    for &p in &list.percentiles {
        header_names.push(format_percentile_header(p));
    }
    header_names.push("Total".to_string());
    header_names.push("% Total".to_string());

    let header_cells: Vec<Cell> = header_names
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

    for entry in &list.data {
        let mut row_cells = Vec::new();

        let short_name = shorten_function_name(&entry.name);
        row_cells.push(Cell::new(&short_name));
        row_cells.push(Cell::new(&entry.calls.to_string()));
        row_cells.push(Cell::new(&entry.avg));

        for &p in &list.percentiles {
            let key = format_percentile_key(p);
            let value = entry
                .percentiles
                .get(&key)
                .map(|s| s.as_str())
                .unwrap_or("N/A");
            row_cells.push(Cell::new(value));
        }

        row_cells.push(Cell::new(&entry.total));
        row_cells.push(Cell::new(&entry.percent_total));

        table.add_row(Row::new(row_cells));
    }

    let mode = list.profiling_mode.to_string();
    let desc = &list.description;
    if list.displayed_count < list.total_count {
        let _ = writeln!(
            writer,
            "{} - {} ({}/{})",
            mode, desc, list.displayed_count, list.total_count
        );
    } else {
        write_section_header(writer, &mode, desc);
        let _ = writeln!(writer);
    }

    if let Some(total_alloc) = &list.total_allocated {
        let _ = writeln!(writer, "Total: {}", total_alloc);
    }

    print_table(&table, writer);

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
