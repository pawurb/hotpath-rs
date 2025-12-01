pub(crate) mod inspect;
pub(crate) mod logs;

use super::super::app::{App, FunctionsFocus};
use super::common_styles;
use ratatui::{
    layout::{Alignment, Constraint, Rect},
    style::{Color, Modifier, Style},
    symbols::border,
    text::Span,
    widgets::{Block, Cell, Paragraph, Row, Table},
    Frame,
};

#[cfg_attr(feature = "hotpath", hotpath::measure)]
pub(crate) fn render_functions_table(frame: &mut Frame, app: &mut App, area: Rect) {
    let title = format!(
        " {} - {} ",
        app.memory_functions.caller_name, app.memory_functions.description
    );

    // Check if memory profiling is available
    if !app.memory_available {
        let message = vec![
            Span::from(""),
            Span::from("Memory profiling is not available.").style(
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::from(""),
            Span::from("To enable memory profiling, run your application with:"),
            Span::from(""),
            Span::from("  cargo run --features hotpath,hotpath-alloc").style(
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

    let header_cells = vec![
        "Function".to_string(),
        "Calls".to_string(),
        "Avg".to_string(),
    ]
    .into_iter()
    .chain(
        app.memory_functions
            .percentiles
            .iter()
            .map(|p| format!("P{}", p))
            .collect::<Vec<_>>(),
    )
    .chain(vec!["Total".to_string(), "% Total".to_string()])
    .map(|h| Cell::from(h).style(common_styles::HEADER_STYLE_CYAN))
    .collect::<Vec<_>>();

    let header = Row::new(header_cells).height(1);

    let entries = app.get_memory_measurements();
    let total_functions = entries.len();
    let function_position = app
        .memory_table_state
        .selected()
        .map(|s| s + 1)
        .unwrap_or(0);

    let rows = entries.iter().map(|(function_name, metrics)| {
        let short_name = hotpath::shorten_function_name(function_name);

        let cells = std::iter::once(Cell::from(short_name))
            .chain(metrics.iter().map(|m| Cell::from(format!("{}", m))))
            .collect::<Vec<_>>();

        Row::new(cells)
    });

    let show_logs = app.show_function_logs;
    let focus = app.functions_focus;

    let num_percentiles = app.memory_functions.percentiles.len();

    let function_pct: u16 = 35;
    let remaining_pct: u16 = 100 - function_pct;
    let num_other_cols = (4 + num_percentiles) as u16; // Calls, Avg, P95s, Total, % Total
    let col_pct: u16 = remaining_pct / num_other_cols;

    let table = Table::new(
        rows,
        vec![Constraint::Percentage(function_pct)] // Function
            .into_iter()
            .chain(vec![
                Constraint::Percentage(col_pct), // Calls
                Constraint::Percentage(col_pct), // Avg
            ])
            .chain((0..num_percentiles).map(|_| Constraint::Percentage(col_pct))) // P95, etc
            .chain(vec![
                Constraint::Percentage(col_pct), // Total
                Constraint::Percentage(col_pct), // % Total
            ])
            .collect::<Vec<_>>(),
    )
    .header(header)
    .block(if show_logs {
        let border_set = if focus == FunctionsFocus::Functions {
            border::THICK
        } else {
            border::PLAIN
        };
        Block::bordered()
            .title(format!(" [{}/{}] ", function_position, total_functions))
            .title(Span::styled(title, common_styles::TITLE_STYLE_YELLOW))
            .border_set(border_set)
            .border_style(if focus == FunctionsFocus::Functions {
                Style::default()
            } else {
                common_styles::UNFOCUSED_BORDER_STYLE
            })
    } else {
        Block::bordered()
            .title(format!(" [{}/{}] ", function_position, total_functions))
            .title(Span::styled(title, common_styles::TITLE_STYLE_YELLOW))
            .border_set(border::THICK)
    })
    .row_highlight_style(common_styles::SELECTED_ROW_STYLE)
    .highlight_symbol(">> ");

    frame.render_stateful_widget(table, area, &mut app.memory_table_state);
}
