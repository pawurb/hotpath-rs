use super::super::app::App;
use ratatui::{
    layout::{Constraint, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Cell, Row, Table},
    Frame,
};

pub(crate) fn render_channels_table(frame: &mut Frame, app: &App, area: Rect) {
    let channels = &app.channels.channels;

    if channels.is_empty() {
        let empty_table = Table::new(
            vec![Row::new(vec![Cell::from("No channels available")])],
            [Constraint::Percentage(100)],
        )
        .block(Block::default().borders(Borders::ALL).title(" Channels "));
        frame.render_widget(empty_table, area);
        return;
    }

    // Define headers
    let headers = Row::new(vec![
        Cell::from("ID"),
        Cell::from("Label"),
        Cell::from("Type"),
        Cell::from("State"),
        Cell::from("Sent"),
        Cell::from("Received"),
        Cell::from("Queued"),
        Cell::from("Type Name"),
    ])
    .style(
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
    );

    // Create rows from channel data
    let rows: Vec<Row> = channels
        .iter()
        .map(|channel| {
            Row::new(vec![
                Cell::from(channel.id.to_string()),
                Cell::from(channel.label.clone()),
                Cell::from(channel.channel_type.to_string()),
                Cell::from(channel.state.to_string()),
                Cell::from(channel.sent_count.to_string()),
                Cell::from(channel.received_count.to_string()),
                Cell::from(channel.queued.to_string()),
                Cell::from(channel.type_name.clone()),
            ])
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Length(8),  // ID
            Constraint::Min(15),    // Label
            Constraint::Length(15), // Type
            Constraint::Length(10), // State
            Constraint::Length(8),  // Sent
            Constraint::Length(10), // Received
            Constraint::Length(8),  // Queued
            Constraint::Min(20),    // Type Name
        ],
    )
    .header(headers)
    .block(Block::default().borders(Borders::ALL).title(" Channels "))
    .column_spacing(1);

    frame.render_widget(table, area);
}
