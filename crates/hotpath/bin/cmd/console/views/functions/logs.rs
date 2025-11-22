use super::super::super::app::App;
use super::super::super::widgets::formatters::format_time_ago;
use ratatui::{
    layout::{Constraint, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::block::BorderType,
    widgets::{Block, Cell, List, ListItem, Row, Table},
    Frame,
};

pub(crate) fn render_function_logs_panel(frame: &mut Frame, area: Rect, app: &App) {
    let title = if let Some(ref function_logs) = app.current_function_logs {
        format!(" {} ", function_logs.function_name)
    } else if app.selected_function_name().is_some() {
        " Loading... ".to_string()
    } else {
        " Recent Logs ".to_string()
    };

    let border_type = BorderType::Plain;
    let block_style = Style::default();

    let block = Block::bordered()
        .border_type(border_type)
        .style(block_style)
        .title(Span::styled(
            title,
            Style::default()
                .fg(Color::Magenta)
                .add_modifier(Modifier::BOLD),
        ));

    if let Some(ref function_logs_data) = app.current_function_logs {
        let is_alloc_mode = matches!(
            app.functions.hotpath_profiling_mode,
            hotpath::ProfilingMode::Alloc
        );

        let headers = if is_alloc_mode {
            Row::new(vec![
                Cell::from("Index").style(
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                Cell::from("Mem").style(
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                Cell::from("Objects").style(
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
            ])
        } else {
            Row::new(vec![
                Cell::from("Index").style(
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                Cell::from("Latency").style(
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
            ])
        };

        let rows: Vec<Row> = function_logs_data
            .logs
            .iter()
            .enumerate()
            .map(|(idx, &(value, elapsed_nanos, count, tid))| {
                let total_elapsed = app.functions.total_elapsed;
                let time_ago_str = if total_elapsed >= elapsed_nanos {
                    let nanos_ago = total_elapsed - elapsed_nanos;
                    format_time_ago(nanos_ago)
                } else {
                    "now".to_string()
                };

                if is_alloc_mode {
                    let mem_str = hotpath::format_bytes(value);
                    let obj_str = count.map_or("0".to_string(), |c| c.to_string());

                    Row::new(vec![
                        Cell::from(format!("{}", idx + 1)),
                        Cell::from(mem_str),
                        Cell::from(obj_str),
                        Cell::from(time_ago_str),
                        Cell::from(tid.to_string()),
                    ])
                } else {
                    let time_str = hotpath::format_duration(value);

                    Row::new(vec![
                        Cell::from(format!("{}", idx + 1)),
                        Cell::from(time_str),
                        Cell::from(time_ago_str),
                        Cell::from(tid.to_string()),
                    ])
                }
            })
            .collect();

        let widths = if is_alloc_mode {
            [
                Constraint::Length(7),  // Index column
                Constraint::Min(10),    // Mem column
                Constraint::Length(9),  // Objects column
                Constraint::Length(12), // Ago column
                Constraint::Length(10), // TID column
            ]
            .as_slice()
        } else {
            [
                Constraint::Length(7),  // Index column
                Constraint::Min(15),    // Latency column (flexible)
                Constraint::Length(12), // Ago column
                Constraint::Length(10), // TID column
            ]
            .as_slice()
        };

        let table = Table::new(rows, widths)
            .header(headers)
            .block(block)
            .column_spacing(2);

        frame.render_widget(table, area);
    } else if app.selected_function_name().is_some() {
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
