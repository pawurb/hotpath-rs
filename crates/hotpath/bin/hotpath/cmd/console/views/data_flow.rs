pub(crate) mod inspect;
pub(crate) mod logs;

use crate::cmd::console::app::DataFlowFocus;
use crate::cmd::console::views::common_styles;
use crate::cmd::console::widgets::formatters::truncate_left;
use hotpath::json::{JsonChannelEntry, JsonFutureEntry, JsonStreamEntry};
use ratatui::{
    layout::{Constraint, Rect},
    style::{Color, Style},
    symbols::border,
    text::Span,
    widgets::{Block, Cell, HighlightSpacing, Row, Table, TableState},
    Frame,
};

fn channel_capacity(channel_type: &str) -> String {
    match channel_type {
        "unbounded" => "∞".to_string(),
        "oneshot" => "1".to_string(),
        s if s.starts_with("bounded") => {
            if let Some(start) = s.find('[') {
                if let Some(end) = s.find(']') {
                    s[start + 1..end].to_string()
                } else {
                    s[start + 1..].to_string()
                }
            } else {
                "?".to_string()
            }
        }
        _ => "?".to_string(),
    }
}

fn state_style(state: &str) -> Style {
    match state {
        "active" | "Active" => Style::default().fg(Color::Green),
        "closed" | "Closed" => Style::default().fg(Color::Yellow),
        "Ready" => Style::default().fg(Color::Green),
        "Cancelled" => Style::default().fg(Color::Red),
        "Suspended" => Style::default().fg(Color::Yellow),
        "Running" => Style::default().fg(Color::Blue),
        "Pending" => Style::default().fg(Color::DarkGray),
        _ => Style::default(),
    }
}

fn list_block(
    title: &'static str,
    show_logs: bool,
    focus: DataFlowFocus,
    position: usize,
    total: usize,
) -> Block<'static> {
    if show_logs {
        let border_set = if focus == DataFlowFocus::List {
            border::THICK
        } else {
            border::PLAIN
        };
        Block::bordered()
            .title(format!(" [{}/{}] ", position, total))
            .title(Span::styled(title, common_styles::TITLE_STYLE_YELLOW))
            .border_set(border_set)
            .border_style(if focus == DataFlowFocus::List {
                Style::default()
            } else {
                common_styles::UNFOCUSED_BORDER_STYLE
            })
    } else {
        Block::bordered()
            .title(format!(" [{}/{}] ", position, total))
            .title(Span::styled(title, common_styles::TITLE_STYLE_YELLOW))
            .border_set(border::THICK)
    }
}

#[hotpath::measure]
#[allow(clippy::too_many_arguments)]
pub(crate) fn render_channels_panel(
    entries: &[JsonChannelEntry],
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
        Cell::from("Sent/Recv"),
    ])
    .style(common_styles::HEADER_STYLE_CYAN)
    .height(1);

    let rows: Vec<Row> = entries
        .iter()
        .map(|entry| {
            let type_text = format!("Channel[{}]", channel_capacity(&entry.channel_type));
            Row::new(vec![
                Cell::from(type_text).style(Style::default().fg(Color::Cyan)),
                Cell::from(truncate_left(&entry.label, label_width)),
                Cell::from(entry.state.clone()).style(state_style(&entry.state)),
                Cell::from(format!("{}/{}", entry.sent_count, entry.received_count)),
            ])
        })
        .collect();

    let widths = [
        Constraint::Length(12),
        Constraint::Percentage(55),
        Constraint::Length(10),
        Constraint::Length(14),
    ];

    let table = Table::new(rows, widths)
        .header(header)
        .block(list_block(
            " Channels - message flow ",
            show_logs,
            focus,
            position,
            total,
        ))
        .column_spacing(1)
        .row_highlight_style(common_styles::SELECTED_ROW_STYLE)
        .highlight_symbol(">> ")
        .highlight_spacing(HighlightSpacing::Always);

    frame.render_stateful_widget(table, area, table_state);
}

#[hotpath::measure]
#[allow(clippy::too_many_arguments)]
pub(crate) fn render_streams_panel(
    entries: &[JsonStreamEntry],
    area: Rect,
    frame: &mut Frame,
    table_state: &mut TableState,
    show_logs: bool,
    focus: DataFlowFocus,
    position: usize,
    total: usize,
) {
    let available_width = area.width.saturating_sub(10);
    let label_width = ((available_width as f32 * 0.6) as usize).max(20);

    let header = Row::new(vec![
        Cell::from("Label"),
        Cell::from("State"),
        Cell::from("Yielded"),
    ])
    .style(common_styles::HEADER_STYLE_CYAN)
    .height(1);

    let rows: Vec<Row> = entries
        .iter()
        .map(|entry| {
            Row::new(vec![
                Cell::from(truncate_left(&entry.label, label_width)),
                Cell::from(entry.state.clone()).style(state_style(&entry.state)),
                Cell::from(entry.items_yielded.to_string()),
            ])
        })
        .collect();

    let widths = [
        Constraint::Percentage(70),
        Constraint::Length(10),
        Constraint::Length(12),
    ];

    let table = Table::new(rows, widths)
        .header(header)
        .block(list_block(
            " Streams - items yielded ",
            show_logs,
            focus,
            position,
            total,
        ))
        .column_spacing(1)
        .row_highlight_style(common_styles::SELECTED_ROW_STYLE)
        .highlight_symbol(">> ")
        .highlight_spacing(HighlightSpacing::Always);

    frame.render_stateful_widget(table, area, table_state);
}

#[hotpath::measure]
#[allow(clippy::too_many_arguments)]
pub(crate) fn render_futures_panel(
    entries: &[JsonFutureEntry],
    area: Rect,
    frame: &mut Frame,
    table_state: &mut TableState,
    show_logs: bool,
    focus: DataFlowFocus,
    position: usize,
    total: usize,
) {
    let available_width = area.width.saturating_sub(10);
    let label_width = ((available_width as f32 * 0.55) as usize).max(20);

    let header = Row::new(vec![
        Cell::from("Label"),
        Cell::from("Calls"),
        Cell::from("Polls"),
        Cell::from("Avg Poll"),
    ])
    .style(common_styles::HEADER_STYLE_CYAN)
    .height(1);

    let rows: Vec<Row> = entries
        .iter()
        .map(|entry| {
            let display_label = hotpath::shorten_function_name(&entry.label);
            let avg_poll_ns = entry
                .total_poll_duration_ns
                .checked_div(entry.total_polls)
                .unwrap_or(0);
            Row::new(vec![
                Cell::from(truncate_left(&display_label, label_width)),
                Cell::from(entry.call_count.to_string()),
                Cell::from(entry.total_polls.to_string()),
                Cell::from(hotpath::format_duration(avg_poll_ns)),
            ])
        })
        .collect();

    let widths = [
        Constraint::Percentage(55),
        Constraint::Length(10),
        Constraint::Length(10),
        Constraint::Length(12),
    ];

    let table = Table::new(rows, widths)
        .header(header)
        .block(list_block(
            " Futures - poll lifecycle ",
            show_logs,
            focus,
            position,
            total,
        ))
        .column_spacing(1)
        .row_highlight_style(common_styles::SELECTED_ROW_STYLE)
        .highlight_symbol(">> ")
        .highlight_spacing(HighlightSpacing::Always);

    frame.render_stateful_widget(table, area, table_state);
}
