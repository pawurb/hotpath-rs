pub(crate) mod inspect;
pub(crate) mod logs;

use crate::cmd::console::app::DataFlowFocus;
use crate::cmd::console::views::common_styles;
use crate::cmd::console::widgets::formatters::truncate_left;
use hotpath::json::{DataFlowType, JsonDataFlowEntry};
use ratatui::{
    layout::{Constraint, Rect},
    style::{Color, Style},
    symbols::border,
    widgets::{Block, Cell, HighlightSpacing, Row, Table, TableState},
    Frame,
};

fn format_type_with_capacity(entry: &JsonDataFlowEntry) -> String {
    match entry.data_flow_type {
        DataFlowType::Channel => {
            let capacity = match entry.subtype.as_deref() {
                Some("unbounded") => "∞".to_string(),
                Some("oneshot") => "1".to_string(),
                Some(s) if s.starts_with("bounded") => {
                    // Extract number from "bounded[N]" or "bounded"
                    if let Some(start) = s.find('[') {
                        if let Some(end) = s.find(']') {
                            s[start + 1..end].to_string()
                        } else {
                            // Handle truncated case like "bounded[10"
                            s[start + 1..].to_string()
                        }
                    } else {
                        "?".to_string()
                    }
                }
                _ => "?".to_string(),
            };
            format!("Channel[{}]", capacity)
        }
        DataFlowType::Stream => "Stream".to_string(),
        DataFlowType::Future => "Future".to_string(),
    }
}

fn type_color(data_flow_type: DataFlowType) -> Color {
    match data_flow_type {
        DataFlowType::Channel => Color::Cyan,
        DataFlowType::Stream => Color::Magenta,
        DataFlowType::Future => Color::Blue,
    }
}

fn state_style(state: &str) -> Style {
    match state {
        "active" | "Active" => Style::default().fg(Color::Green),
        "closed" | "Closed" => Style::default().fg(Color::Yellow),
        "full" => Style::default().fg(Color::Red),
        "Ready" => Style::default().fg(Color::Green),
        "Cancelled" => Style::default().fg(Color::Red),
        "Suspended" => Style::default().fg(Color::Yellow),
        "Running" => Style::default().fg(Color::Blue),
        "Pending" => Style::default().fg(Color::DarkGray),
        _ => Style::default(),
    }
}

fn format_counts(entry: &JsonDataFlowEntry) -> String {
    match entry.data_flow_type {
        DataFlowType::Channel => {
            if let Some(received) = entry.secondary_count {
                format!("{}/{}", entry.primary_count, received)
            } else {
                entry.primary_count.to_string()
            }
        }
        DataFlowType::Stream => entry.primary_count.to_string(),
        DataFlowType::Future => entry.primary_count.to_string(),
    }
}

fn format_queue(entry: &JsonDataFlowEntry) -> &str {
    entry.queue.as_deref().unwrap_or("-")
}

fn format_queue_mem(entry: &JsonDataFlowEntry) -> &str {
    entry.queue_mem.as_deref().unwrap_or("-")
}

#[hotpath::measure]
#[allow(clippy::too_many_arguments)]
pub(crate) fn render_data_flow_panel(
    entries: &[JsonDataFlowEntry],
    area: Rect,
    frame: &mut Frame,
    table_state: &mut TableState,
    show_logs: bool,
    focus: DataFlowFocus,
    position: usize,
    total: usize,
) {
    let available_width = area.width.saturating_sub(10);
    let label_width = ((available_width as f32 * 0.45) as usize).max(20);

    let header = Row::new(vec![
        Cell::from("Type"),
        Cell::from("Label"),
        Cell::from("State"),
        Cell::from("Queue"),
        Cell::from("Mem"),
        Cell::from("Count"),
    ])
    .style(common_styles::HEADER_STYLE)
    .height(1);

    let rows: Vec<Row> = entries
        .iter()
        .map(|entry| {
            let type_text = format_type_with_capacity(entry);
            let type_cell =
                Cell::from(type_text).style(Style::default().fg(type_color(entry.data_flow_type)));

            let state_text = if entry.state == "full" {
                format!("⚠ {}", entry.state)
            } else {
                entry.state.clone()
            };

            Row::new(vec![
                type_cell,
                Cell::from(truncate_left(&entry.label, label_width)),
                Cell::from(state_text).style(state_style(&entry.state)),
                Cell::from(format_queue(entry)),
                Cell::from(format_queue_mem(entry)),
                Cell::from(format_counts(entry)),
            ])
        })
        .collect();

    let widths = [
        Constraint::Length(12),     // Type (Channel[∞], Channel[100], etc.)
        Constraint::Percentage(40), // Label
        Constraint::Length(10),     // State
        Constraint::Length(8),      // Queue
        Constraint::Length(8),      // Mem
        Constraint::Length(12),     // Count
    ];

    let table_block = if show_logs {
        let border_set = if focus == DataFlowFocus::List {
            border::THICK
        } else {
            border::PLAIN
        };
        Block::bordered()
            .title(format!(" [{}/{}] ", position, total))
            .border_set(border_set)
            .border_style(if focus == DataFlowFocus::List {
                Style::default()
            } else {
                common_styles::UNFOCUSED_BORDER_STYLE
            })
    } else {
        Block::bordered()
            .title(format!(" [{}/{}] ", position, total))
            .border_set(border::THICK)
    };

    let table = Table::new(rows, widths)
        .header(header)
        .block(table_block)
        .column_spacing(1)
        .row_highlight_style(common_styles::SELECTED_ROW_STYLE)
        .highlight_symbol(">> ")
        .highlight_spacing(HighlightSpacing::Always);

    frame.render_stateful_widget(table, area, table_state);
}
