use super::super::app::App;
use super::super::widgets::formatters::format_time_ago;
use ratatui::{
    layout::{Constraint, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::block::BorderType,
    widgets::{Block, Cell, List, ListItem, Row, Table},
    Frame,
};

pub(crate) fn render_samples_panel(frame: &mut Frame, area: Rect, app: &App) {
    let title = if let Some(ref samples) = app.current_samples {
        format!(" {} ", samples.function_name)
    } else if app.selected_function_name().is_some() {
        " Loading... ".to_string()
    } else {
        " Recent Samples ".to_string()
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

    if let Some(ref samples_data) = app.current_samples {
        let headers = Row::new(vec![
            Cell::from("Index").style(
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Cell::from("Metric").style(
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Cell::from("Ago").style(
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
        ]);

        let rows: Vec<Row> = samples_data
            .samples
            .iter()
            .enumerate()
            .map(|(idx, &(value, elapsed_nanos))| {
                let formatted_value =
                    format_sample_value(value, &app.metrics.hotpath_profiling_mode);

                let total_elapsed = app.metrics.total_elapsed;
                let time_ago_str = if total_elapsed >= elapsed_nanos {
                    let nanos_ago = total_elapsed - elapsed_nanos;
                    format_time_ago(nanos_ago)
                } else {
                    "now".to_string()
                };

                Row::new(vec![
                    Cell::from(format!("{}", idx + 1)).style(Style::default().fg(Color::Green)),
                    Cell::from(formatted_value).style(Style::default().fg(Color::Cyan)),
                    Cell::from(time_ago_str).style(Style::default().fg(Color::DarkGray)),
                ])
            })
            .collect();

        let widths = [
            Constraint::Length(7),  // Index column
            Constraint::Min(15),    // Metric column (flexible)
            Constraint::Length(12), // Ago column
        ];

        let table = Table::new(rows, widths)
            .header(headers)
            .block(block)
            .column_spacing(2);

        frame.render_widget(table, area);
    } else if app.selected_function_name().is_some() {
        // No samples yet
        let items = vec![
            ListItem::new(Line::from("")),
            ListItem::new(Line::from(Span::styled(
                "  Loading samples...",
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
                "  Navigate the function list to see samples.",
                Style::default().fg(Color::DarkGray),
            ))),
        ];
        let list = List::new(items).block(block);
        frame.render_widget(list, area);
    }
}

fn format_sample_value(value: u64, profiling_mode: &hotpath::ProfilingMode) -> String {
    match profiling_mode {
        hotpath::ProfilingMode::Timing => hotpath::format_duration(value),
        hotpath::ProfilingMode::AllocBytesTotal => hotpath::format_bytes(value),
        hotpath::ProfilingMode::AllocCountTotal => {
            format!("{}", value)
        }
    }
}
