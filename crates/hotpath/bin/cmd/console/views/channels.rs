pub(crate) mod inspect;
pub(crate) mod logs;

use super::common_styles;
use crate::cmd::console::app::ChannelsFocus;
use crate::cmd::console::widgets::formatters::{queue_status, truncate_left};
use hotpath::channels::{ChannelState, ChannelType, SerializableChannelStats};
use hotpath::format_bytes;
use ratatui::{
    layout::{Constraint, Rect},
    style::{Color, Style},
    symbols::border,
    widgets::{Block, Cell, HighlightSpacing, Row, Table, TableState},
    Frame,
};

/// Renders the channels table with channel statistics
#[allow(clippy::too_many_arguments)]
pub(crate) fn render_channels_panel(
    stats: &[SerializableChannelStats],
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
            let (state_text, state_style) = match stat.state {
                ChannelState::Active => (stat.state.to_string(), Style::default().fg(Color::Green)),
                ChannelState::Closed => {
                    (stat.state.to_string(), Style::default().fg(Color::Yellow))
                }
                ChannelState::Full => {
                    (format!("⚠ {}", stat.state), Style::default().fg(Color::Red))
                }
                ChannelState::Notified => {
                    (stat.state.to_string(), Style::default().fg(Color::Blue))
                }
            };

            let mem_cell = match &stat.channel_type {
                ChannelType::Unbounded => Cell::from("N/A"),
                _ => Cell::from(format_bytes(stat.queued_bytes)),
            };
            let queue_cell = queue_status(stat.queued, &stat.channel_type, 8);

            Row::new(vec![
                Cell::from(truncate_left(&stat.label, channel_width)),
                Cell::from(stat.channel_type.to_string()),
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
