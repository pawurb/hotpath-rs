pub(crate) mod inspect;
pub(crate) mod logs;

use super::common_styles;
use crate::cmd::console::app::StreamsFocus;
use crate::cmd::console::widgets::formatters::truncate_left;
use hotpath::channels::ChannelState;
use hotpath::streams::SerializableStreamStats;
use ratatui::{
    layout::{Constraint, Rect},
    style::{Color, Style},
    symbols::border,
    widgets::{Block, Cell, HighlightSpacing, Row, Table, TableState},
    Frame,
};

/// Renders the streams table with stream statistics
#[allow(clippy::too_many_arguments)]
pub(crate) fn render_streams_panel(
    stats: &[SerializableStreamStats],
    area: Rect,
    frame: &mut Frame,
    table_state: &mut TableState,
    show_logs: bool,
    focus: StreamsFocus,
    stream_position: usize,
    total_streams: usize,
) {
    let available_width = area.width.saturating_sub(10);
    let stream_width = ((available_width as f32 * 0.60) as usize).max(36);

    let header = Row::new(vec![
        Cell::from("Stream"),
        Cell::from("State"),
        Cell::from("Yielded"),
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
                _ => (stat.state.to_string(), Style::default().fg(Color::Gray)),
            };

            Row::new(vec![
                Cell::from(truncate_left(&stat.label, stream_width)),
                Cell::from(state_text).style(state_style),
                Cell::from(stat.items_yielded.to_string()),
            ])
        })
        .collect();

    let widths = [
        Constraint::Percentage(60), // Stream
        Constraint::Percentage(20), // State
        Constraint::Percentage(20), // Yielded
    ];

    let table_block = if show_logs {
        let border_set = if focus == StreamsFocus::Streams {
            border::THICK
        } else {
            border::PLAIN
        };
        Block::bordered()
            .title(format!(" [{}/{}] ", stream_position, total_streams))
            .border_set(border_set)
            .border_style(if focus == StreamsFocus::Streams {
                Style::default()
            } else {
                common_styles::UNFOCUSED_BORDER_STYLE
            })
    } else {
        Block::bordered()
            .title(format!(" [{}/{}] ", stream_position, total_streams))
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
