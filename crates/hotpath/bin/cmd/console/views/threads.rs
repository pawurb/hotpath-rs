use super::common_styles;
use crate::cmd::console::widgets::formatters::truncate_left;
use hotpath::threads::ThreadMetrics;
use ratatui::{
    layout::{Constraint, Rect},
    symbols::border,
    widgets::{Block, Cell, HighlightSpacing, Row, Table, TableState},
    Frame,
};

/// Renders the threads table with thread metrics
#[cfg_attr(feature = "hotpath", hotpath::measure)]
#[allow(clippy::too_many_arguments)]
pub(crate) fn render_threads_panel(
    threads: &[ThreadMetrics],
    area: Rect,
    frame: &mut Frame,
    table_state: &mut TableState,
    thread_position: usize,
    total_threads: usize,
) {
    let available_width = area.width.saturating_sub(10);
    let thread_width = ((available_width as f32 * 0.50) as usize).max(20);

    let header = Row::new(vec![
        Cell::from("Thread"),
        Cell::from("TID"),
        Cell::from("CPU %"),
        Cell::from("User"),
        Cell::from("Sys"),
    ])
    .style(common_styles::HEADER_STYLE)
    .height(1);

    let rows: Vec<Row> = threads
        .iter()
        .map(|thread| {
            let cpu_percent_str = match thread.cpu_percent {
                Some(pct) => format!("{:.1}%", pct),
                None => "-".to_string(),
            };

            Row::new(vec![
                Cell::from(truncate_left(&thread.name, thread_width)),
                Cell::from(thread.os_tid.to_string()),
                Cell::from(cpu_percent_str),
                Cell::from(format!("{:.2}s", thread.cpu_user)),
                Cell::from(format!("{:.2}s", thread.cpu_sys)),
            ])
        })
        .collect();

    let widths = [
        Constraint::Percentage(30), // Thread name
        Constraint::Percentage(10), // TID
        Constraint::Percentage(15), // CPU %
        Constraint::Percentage(12), // User
        Constraint::Percentage(13), // Sys
    ];

    let table_block = Block::bordered()
        .title(format!(" [{}/{}] ", thread_position, total_threads))
        .border_set(border::THICK);

    let table = Table::new(rows, widths)
        .header(header)
        .block(table_block)
        .column_spacing(1)
        .row_highlight_style(common_styles::SELECTED_ROW_STYLE)
        .highlight_symbol(">> ")
        .highlight_spacing(HighlightSpacing::Always);

    frame.render_stateful_widget(table, area, table_state);
}
