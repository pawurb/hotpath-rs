use crate::cmd::console::app::App;
use crate::cmd::console::views::common_styles;
use ratatui::{
    layout::{Alignment, Constraint, Rect},
    style::{Color, Modifier, Style},
    symbols::border,
    text::Span,
    widgets::{Block, Cell, Paragraph, Row, Table},
    Frame,
};

#[hotpath::measure]
pub(crate) fn render_functions_table(frame: &mut Frame, app: &mut App, area: Rect) {
    let title = format!(
        " {} - {} ",
        app.cpu_functions.caller_name, app.cpu_functions.description
    );

    if !app.cpu_available {
        let message = vec![
            Span::from(""),
            Span::from("CPU profiling is not available.").style(
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::from(""),
            Span::from("To enable CPU profiling, run your application with:"),
            Span::from(""),
            Span::from("  cargo run --features hotpath,cpu").style(
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::from(""),
        ];

        let block = Block::bordered()
            .border_set(border::THICK)
            .title(Span::styled(title, common_styles::TITLE_STYLE_YELLOW));

        let paragraph = Paragraph::new(
            message
                .into_iter()
                .map(ratatui::text::Line::from)
                .collect::<Vec<_>>(),
        )
        .block(block)
        .alignment(Alignment::Center);

        frame.render_widget(paragraph, area);
        return;
    }

    let header_cells = vec!["Function", "Samples", "% Total"]
        .into_iter()
        .map(|h| Cell::from(h).style(common_styles::HEADER_STYLE_CYAN))
        .collect::<Vec<_>>();
    let header = Row::new(header_cells).height(1);

    let entries = &app.cpu_functions.data;
    let total_functions = entries.len();
    let function_position = app.cpu_table_state.selected().map(|s| s + 1).unwrap_or(0);

    let rows = entries.iter().map(|func| {
        let short_name = hotpath::shorten_function_name(&func.name);
        Row::new(vec![
            Cell::from(short_name),
            Cell::from(func.samples.to_string()),
            Cell::from(func.percent.clone()),
        ])
    });

    let table = Table::new(
        rows,
        vec![
            Constraint::Percentage(60),
            Constraint::Percentage(20),
            Constraint::Percentage(20),
        ],
    )
    .header(header)
    .block(
        Block::bordered()
            .title(format!(" [{}/{}] ", function_position, total_functions))
            .title(Span::styled(title, common_styles::TITLE_STYLE_YELLOW))
            .border_set(border::THICK),
    )
    .row_highlight_style(common_styles::SELECTED_ROW_STYLE)
    .highlight_symbol(">> ");

    frame.render_stateful_widget(table, area, &mut app.cpu_table_state);
}
