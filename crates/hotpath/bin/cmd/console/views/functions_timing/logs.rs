use super::super::super::widgets::formatters::format_time_ago;
use hotpath::{FunctionLogsJson, ProfilingMode};
use ratatui::{
    layout::{Constraint, Rect},
    style::{Color, Modifier, Style},
    symbols::border,
    text::{Line, Span},
    widgets::{Block, Cell, HighlightSpacing, List, ListItem, Row, Table, TableState},
    Frame,
};

#[allow(clippy::too_many_arguments)]
pub(crate) fn render_function_logs_panel(
    current_function_logs: Option<&FunctionLogsJson>,
    selected_function_name: Option<&str>,
    _profiling_mode: &ProfilingMode,
    total_elapsed: u64,
    area: Rect,
    frame: &mut Frame,
    table_state: &mut TableState,
    is_focused: bool,
) {
    let title = if let Some(function_logs) = current_function_logs {
        format!(" {} ", function_logs.function_name)
    } else if selected_function_name.is_some() {
        " Loading... ".to_string()
    } else {
        " Recent Logs ".to_string()
    };

    let border_set = if is_focused {
        border::THICK
    } else {
        border::PLAIN
    };

    let block = Block::bordered()
        .border_set(border_set)
        .border_style(if is_focused {
            Style::default()
        } else {
            Style::default().fg(Color::DarkGray)
        })
        .title(Span::styled(
            title,
            Style::default()
                .fg(Color::Magenta)
                .add_modifier(Modifier::BOLD),
        ));

    if let Some(function_logs_data) = current_function_logs {
        // Timing tab always shows timing/latency data
        let headers = Row::new(vec![
            Cell::from("Index").style(
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Cell::from("Timing").style(
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Cell::from("Ago").style(
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Cell::from("TID").style(
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Cell::from("Result").style(
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
        ]);

        let total_invocations = function_logs_data.count;

        let rows: Vec<Row> = function_logs_data
            .logs
            .iter()
            .enumerate()
            .map(|(idx, entry)| {
                let time_ago_str = if total_elapsed >= entry.elapsed_nanos {
                    let nanos_ago = total_elapsed - entry.elapsed_nanos;
                    format_time_ago(nanos_ago)
                } else {
                    "now".to_string()
                };

                let time_str = entry
                    .value
                    .map_or("N/A".to_string(), |v| hotpath::format_duration(v));
                let invocation_number = total_invocations - idx;
                let result_str = entry.result.as_deref().unwrap_or("N/A");

                Row::new(vec![
                    Cell::from(format!("{}", invocation_number)),
                    Cell::from(time_str),
                    Cell::from(time_ago_str),
                    Cell::from(entry.tid.map_or("N/A".to_string(), |t| t.to_string())),
                    Cell::from(result_str.to_string()),
                ])
            })
            .collect();

        let widths = [
            Constraint::Length(7),  // Index column
            Constraint::Length(12), // Timing column
            Constraint::Length(12), // Ago column
            Constraint::Length(10), // TID column
            Constraint::Min(20),    // Result column (flexible)
        ]
        .as_slice();

        let selected_row_style = Style::default()
            .bg(Color::DarkGray)
            .add_modifier(Modifier::BOLD);

        let table = Table::new(rows, widths)
            .header(headers)
            .block(block)
            .column_spacing(2)
            .row_highlight_style(selected_row_style)
            .highlight_symbol(">> ")
            .highlight_spacing(HighlightSpacing::Always);

        frame.render_stateful_widget(table, area, table_state);
    } else if selected_function_name.is_some() {
        // No logs yet
        let items = vec![
            ListItem::new(Line::from("")),
            ListItem::new(Line::from(Span::styled(
                "  Loading logs...",
                Style::default().fg(Color::Gray),
            ))),
        ];
        let list = List::new(items).block(block);
        frame.render_widget(list, area);
    } else {
        // No function selected
        let items = vec![
            ListItem::new(Line::from("")),
            ListItem::new(Line::from(Span::styled(
                "  No function selected",
                Style::default().fg(Color::Gray),
            ))),
            ListItem::new(Line::from("")),
            ListItem::new(Line::from(Span::styled(
                "  Navigate the function list to see logs.",
                Style::default().fg(Color::DarkGray),
            ))),
        ];
        let list = List::new(items).block(block);
        frame.render_widget(list, area);
    }
}
