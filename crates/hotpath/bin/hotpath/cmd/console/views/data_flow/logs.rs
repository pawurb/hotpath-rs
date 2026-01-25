use crate::cmd::console::app::DataFlowLogs;
use crate::cmd::console::views::common_styles;
use crate::cmd::console::widgets::formatters::truncate_message;
use hotpath::json::DataFlowType;
use ratatui::{
    layout::{Constraint, Rect},
    style::{Color, Style},
    symbols::border,
    widgets::{Block, Cell, HighlightSpacing, Row, Table, TableState},
    Frame,
};

pub(crate) fn render_logs_placeholder(label: &str, message: &str, area: Rect, frame: &mut Frame) {
    let block = Block::bordered()
        .title(format!(" {} ", label))
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

pub(crate) fn render_logs_panel(
    logs: &DataFlowLogs,
    data_flow_type: DataFlowType,
    label: &str,
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
        .title(format!(" {} ", label))
        .border_set(border_set)
        .border_style(if is_focused {
            Style::default()
        } else {
            common_styles::UNFOCUSED_BORDER_STYLE
        });

    let inner_area = block.inner(area);
    frame.render_widget(block, area);

    let available_width = inner_area.width.saturating_sub(2);
    let msg_width = (available_width.saturating_sub(30) as usize).max(20);

    match (logs, data_flow_type) {
        (DataFlowLogs::Channel(channel_logs), DataFlowType::Channel) => {
            let header = Row::new(vec!["Index", "Message", "Delay", "Ago"])
                .style(common_styles::HEADER_STYLE)
                .height(1);

            let rows: Vec<Row> = channel_logs
                .sent_logs
                .iter()
                .map(|entry| {
                    let msg = entry.message.as_deref().unwrap_or("");
                    let truncated_msg = truncate_message(msg, msg_width);
                    let delay_str = entry.delay.as_deref().unwrap_or("queued");

                    Row::new(vec![
                        entry.index.to_string(),
                        truncated_msg,
                        delay_str.to_string(),
                        entry.ago.clone(),
                    ])
                })
                .collect();

            let widths = [
                Constraint::Length(6),  // Index
                Constraint::Min(20),    // Message
                Constraint::Length(12), // Delay
                Constraint::Length(13), // Ago
            ];

            let table = Table::new(rows, widths)
                .header(header)
                .row_highlight_style(common_styles::SELECTED_ROW_STYLE)
                .highlight_symbol(">> ")
                .highlight_spacing(HighlightSpacing::Always);

            frame.render_stateful_widget(table, inner_area, table_state);
        }
        (DataFlowLogs::Stream(stream_logs), DataFlowType::Stream) => {
            let header = Row::new(vec!["Index", "Message", "Ago"])
                .style(common_styles::HEADER_STYLE)
                .height(1);

            let rows: Vec<Row> = stream_logs
                .logs
                .iter()
                .map(|entry| {
                    let msg = entry.message.as_deref().unwrap_or("");
                    let truncated_msg = truncate_message(msg, msg_width);

                    Row::new(vec![
                        entry.index.to_string(),
                        truncated_msg,
                        entry.ago.clone(),
                    ])
                })
                .collect();

            let widths = [
                Constraint::Length(6),  // Index
                Constraint::Min(20),    // Message
                Constraint::Length(13), // Ago
            ];

            let table = Table::new(rows, widths)
                .header(header)
                .row_highlight_style(common_styles::SELECTED_ROW_STYLE)
                .highlight_symbol(">> ")
                .highlight_spacing(HighlightSpacing::Always);

            frame.render_stateful_widget(table, inner_area, table_state);
        }
        (DataFlowLogs::Future(future_logs), DataFlowType::Future) => {
            let result_width = (available_width.saturating_sub(25) as usize).max(10);

            let header = Row::new(vec!["ID", "State", "Result", "Polls"])
                .style(common_styles::HEADER_STYLE)
                .height(1);

            let rows: Vec<Row> = future_logs
                .calls
                .iter()
                .map(|call| {
                    let result = call.result.as_deref().unwrap_or("-");
                    let result_text = truncate_message(result, result_width);

                    Row::new(vec![
                        Cell::from(call.id.to_string()),
                        Cell::from(call.state.clone()).style(state_style(&call.state)),
                        Cell::from(result_text),
                        Cell::from(call.poll_count.to_string()),
                    ])
                })
                .collect();

            let widths = [
                Constraint::Length(8), // ID
                Constraint::Length(9), // State
                Constraint::Min(10),   // Result
                Constraint::Length(6), // Polls
            ];

            let table = Table::new(rows, widths)
                .header(header)
                .row_highlight_style(common_styles::SELECTED_ROW_STYLE)
                .highlight_symbol(">> ")
                .highlight_spacing(HighlightSpacing::Always);

            frame.render_stateful_widget(table, inner_area, table_state);
        }
        _ => {}
    }
}
