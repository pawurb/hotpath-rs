use crate::cmd::console::app::CachedStreamLogs;
use crate::cmd::console::widgets::formatters::{format_time_ago, truncate_message};
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    symbols::border,
    text::Text,
    widgets::{Block, HighlightSpacing, Row, Table, TableState},
    Frame,
};

/// Renders a placeholder when no logs are available
pub(crate) fn render_logs_placeholder(
    stream_label: &str,
    message: &str,
    area: Rect,
    frame: &mut Frame,
) {
    let block = Block::bordered()
        .title(format!(" {} ", stream_label))
        .border_set(border::THICK);

    let inner_area = block.inner(area);
    frame.render_widget(block, area);

    let message_width = message.len() as u16;
    let x = inner_area.x + (inner_area.width.saturating_sub(message_width)) / 2;
    let y = inner_area.y + inner_area.height / 2;

    if x < inner_area.x + inner_area.width && y < inner_area.y + inner_area.height {
        frame
            .buffer_mut()
            .set_string(x, y, message, Style::default().fg(Color::DarkGray));
    }
}

/// Renders the logs panel with yielded stream log entries
pub(crate) fn render_logs_panel(
    cached_logs: &CachedStreamLogs,
    stream_label: &str,
    area: Rect,
    frame: &mut Frame,
    table_state: &mut TableState,
    is_focused: bool,
    current_elapsed_ns: u64,
) {
    let border_set = if is_focused {
        border::THICK
    } else {
        border::PLAIN
    };

    let block = Block::bordered()
        .title(format!(" {} ", stream_label))
        .border_set(border_set)
        .style(if is_focused {
            Style::default()
        } else {
            Style::default().fg(Color::DarkGray)
        });

    let inner_area = block.inner(area);
    frame.render_widget(block, area);

    let available_width = inner_area.width.saturating_sub(2);
    let msg_width = (available_width.saturating_sub(20) as usize).max(20);

    let header_style = Style::default()
        .fg(Color::Yellow)
        .add_modifier(Modifier::BOLD);

    let header = Row::new(vec!["Index", "Message", "Ago"])
        .style(header_style)
        .height(1);

    let rows: Vec<Row> = cached_logs
        .logs
        .logs
        .iter()
        .map(|entry| {
            let time_ago = format_time_ago(current_elapsed_ns.saturating_sub(entry.timestamp));

            let msg = entry.message.as_deref().unwrap_or("");
            let truncated_msg = truncate_message(msg, msg_width);

            Row::new(vec![entry.index.to_string(), truncated_msg, time_ago])
        })
        .collect();

    let widths = [
        ratatui::layout::Constraint::Length(6),  // Index
        ratatui::layout::Constraint::Min(20),    // Message
        ratatui::layout::Constraint::Length(13), // Ago
    ];

    let selected_row_style = Style::default()
        .add_modifier(Modifier::REVERSED)
        .bg(Color::DarkGray);

    let table = Table::new(rows, widths)
        .header(header)
        .row_highlight_style(selected_row_style)
        .highlight_symbol(Text::from(">"))
        .highlight_spacing(HighlightSpacing::Always);

    frame.render_stateful_widget(table, inner_area, table_state);
}
