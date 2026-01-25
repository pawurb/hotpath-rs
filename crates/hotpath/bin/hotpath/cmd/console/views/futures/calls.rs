use super::super::common_styles;
use crate::cmd::console::widgets::formatters::truncate_message;
use hotpath::json::{JsonFutureLog, JsonFutureLogsList};
use ratatui::{
    layout::Rect,
    style::{Color, Style},
    symbols::border,
    widgets::{Block, Cell, HighlightSpacing, Row, Table, TableState},
    Frame,
};

pub(crate) fn render_calls_placeholder(
    future_label: &str,
    message: &str,
    area: Rect,
    frame: &mut Frame,
) {
    let block = Block::bordered()
        .title(format!(" {} ", future_label))
        .border_set(border::THICK);

    let inner_area = block.inner(area);
    frame.render_widget(block, area);

    let message_width = message.len() as u16;
    let x = inner_area.x + (inner_area.width.saturating_sub(message_width)) / 2;
    let y = inner_area.y + inner_area.height / 2;

    if x < inner_area.x + inner_area.width && y < inner_area.y + inner_area.height {
        frame
            .buffer_mut()
            .set_string(x, y, message, common_styles::PLACEHOLDER_STYLE);
    }
}

fn state_style(state: &str) -> Style {
    match state {
        "Ready" => Style::default().fg(Color::Green),
        "Cancelled" => Style::default().fg(Color::Red),
        "Suspended" => Style::default().fg(Color::Yellow),
        "Running" => Style::default().fg(Color::Blue),
        "Pending" => Style::default().fg(Color::DarkGray),
        _ => Style::default(),
    }
}

fn render_call_row(call: &JsonFutureLog, result_width: usize) -> Row<'static> {
    let result = call.result.as_deref().unwrap_or("-");
    let result_text = truncate_message(result, result_width);

    Row::new(vec![
        Cell::from(call.id.to_string()),
        Cell::from(call.state.clone()).style(state_style(&call.state)),
        Cell::from(result_text),
        Cell::from(call.poll_count.to_string()),
    ])
}

pub(crate) fn render_calls_panel(
    future_calls: &JsonFutureLogsList,
    future_label: &str,
    area: Rect,
    frame: &mut Frame,
    table_state: &mut TableState,
    is_focused: bool,
) {
    let border_set = if is_focused {
        border::THICK
    } else {
        border::PLAIN
    };

    let block = Block::bordered()
        .title(format!(" {} ", future_label))
        .border_set(border_set)
        .border_style(if is_focused {
            Style::default()
        } else {
            common_styles::UNFOCUSED_BORDER_STYLE
        });

    let inner_area = block.inner(area);
    frame.render_widget(block, area);

    let available_width = inner_area.width.saturating_sub(4);
    let result_width = (available_width.saturating_sub(25) as usize).max(10);

    let header = Row::new(vec!["ID", "State", "Result", "Polls"])
        .style(common_styles::HEADER_STYLE)
        .height(1);

    let rows: Vec<Row> = future_calls
        .calls
        .iter()
        .map(|call| render_call_row(call, result_width))
        .collect();

    let widths = [
        ratatui::layout::Constraint::Length(8), // ID
        ratatui::layout::Constraint::Length(9), // State
        ratatui::layout::Constraint::Min(10),   // Result
        ratatui::layout::Constraint::Length(6), // Polls
    ];

    let table = Table::new(rows, widths)
        .header(header)
        .row_highlight_style(common_styles::SELECTED_ROW_STYLE)
        .highlight_symbol(">> ")
        .highlight_spacing(HighlightSpacing::Always);

    frame.render_stateful_widget(table, inner_area, table_state);
}
