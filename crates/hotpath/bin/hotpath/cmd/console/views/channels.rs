pub(crate) mod inspect;
pub(crate) mod logs;

use super::common_styles;
use crate::cmd::console::app::ChannelsFocus;
use crate::cmd::console::widgets::formatters::truncate_left;
use hotpath::formatted::FormattedChannelStats;
use ratatui::{
    layout::{Constraint, Rect},
    style::{Color, Style},
    symbols::border,
    widgets::{Block, Cell, HighlightSpacing, Row, Table, TableState},
    Frame,
};

fn queue_status_cell(queue_status: &str, queued: u64) -> Cell<'static> {
    if queue_status.contains('∞') {
        return Cell::from("N/A");
    }

    let parts: Vec<&str> = queue_status.split('/').collect();
    if parts.len() != 2 {
        return Cell::from(format!("[{}]", queue_status));
    }

    let capacity: u64 = match parts[1].parse() {
        Ok(c) => c,
        Err(_) => return Cell::from(format!("[{}]", queue_status)),
    };

    if capacity == 0 {
        return Cell::from(format!("[{}]", queue_status));
    }

    let percentage = (queued as f64 / capacity as f64 * 100.0).min(100.0);
    let color = if percentage >= 100.0 {
        Color::Red
    } else if percentage >= 50.0 {
        Color::Yellow
    } else {
        Color::Green
    };

    Cell::from(format!("[{}]", queue_status)).style(Style::default().fg(color))
}

/// Renders the channels table with channel statistics
#[hotpath::measure]
#[allow(clippy::too_many_arguments)]
pub(crate) fn render_channels_panel(
    stats: &[FormattedChannelStats],
    area: Rect,
    frame: &mut Frame,
    table_state: &mut TableState,
    show_logs: bool,
    focus: ChannelsFocus,
    channel_position: usize,
    total_channels: usize,
) {
    let available_width = area.width.saturating_sub(10);
    let channel_width = ((available_width as f32 * 0.22) as usize).max(36);

    let header = Row::new(vec![
        Cell::from("Channel"),
        Cell::from("Type"),
        Cell::from("State"),
        Cell::from("Sent"),
        Cell::from("Receive"),
        Cell::from("Queue"),
        Cell::from("Mem"),
    ])
    .style(common_styles::HEADER_STYLE)
    .height(1);

    let rows: Vec<Row> = stats
        .iter()
        .map(|stat| {
            let state_style = match stat.state.as_str() {
                "active" => Style::default().fg(Color::Green),
                "closed" => Style::default().fg(Color::Yellow),
                "full" => Style::default().fg(Color::Red),
                "notified" => Style::default().fg(Color::Blue),
                _ => Style::default(),
            };

            let state_text = if stat.state == "full" {
                format!("⚠ {}", stat.state)
            } else {
                stat.state.clone()
            };

            let mem_cell = if stat.channel_type == "unbounded" {
                Cell::from("N/A")
            } else {
                Cell::from(stat.queued_bytes.clone())
            };

            let queue_cell = queue_status_cell(&stat.queue_status, stat.queued);

            Row::new(vec![
                Cell::from(truncate_left(&stat.label, channel_width)),
                Cell::from(stat.channel_type.clone()),
                Cell::from(state_text).style(state_style),
                Cell::from(stat.sent_count.to_string()),
                Cell::from(stat.received_count.to_string()),
                queue_cell,
                mem_cell,
            ])
        })
        .collect();

    let widths = [
        Constraint::Percentage(30), // Channel
        Constraint::Percentage(14), // Type
        Constraint::Percentage(10), // State
        Constraint::Percentage(9),  // Sent
        Constraint::Percentage(11), // Received
        Constraint::Percentage(16), // Queue
        Constraint::Percentage(10), // Mem
    ];

    let table_block = if show_logs {
        let border_set = if focus == ChannelsFocus::Channels {
            border::THICK
        } else {
            border::PLAIN
        };
        Block::bordered()
            .title(format!(" [{}/{}] ", channel_position, total_channels))
            .border_set(border_set)
            .border_style(if focus == ChannelsFocus::Channels {
                Style::default()
            } else {
                common_styles::UNFOCUSED_BORDER_STYLE
            })
    } else {
        Block::bordered()
            .title(format!(" [{}/{}] ", channel_position, total_channels))
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
