use crate::cmd::console::views::common_styles;
use crate::cmd::console::widgets::formatters::truncate_message;
use hotpath::json::JsonSqlLogsList;
use ratatui::{
    layout::{Constraint, Rect},
    style::{Color, Modifier, Style},
    symbols::border,
    text::{Line, Span},
    widgets::{Block, HighlightSpacing, Row, Table, TableState},
    Frame,
};

pub(crate) fn render_sql_logs_panel(
    logs: &JsonSqlLogsList,
    label: &str,
    area: Rect,
    frame: &mut Frame,
    table_state: &mut TableState,
    is_focused: bool,
) {
    let title_style = Style::default()
        .fg(Color::Magenta)
        .add_modifier(Modifier::BOLD);
    let title = Line::from(Span::styled(format!(" {} ", label), title_style));

    let border_set = if is_focused {
        border::THICK
    } else {
        border::PLAIN
    };

    let block = Block::bordered()
        .title(title)
        .border_set(border_set)
        .border_style(if is_focused {
            Style::default()
        } else {
            common_styles::UNFOCUSED_BORDER_STYLE
        });

    let inner_area = block.inner(area);
    frame.render_widget(block, area);

    let available_width = inner_area.width.saturating_sub(2);
    let query_width = (available_width.saturating_sub(34) as usize).max(20);

    let header = Row::new(vec!["Index", "Query", "Time", "Ago"])
        .style(common_styles::HEADER_STYLE_CYAN)
        .height(1);

    let rows: Vec<Row> = logs
        .logs
        .iter()
        .map(|entry| {
            Row::new(vec![
                entry.index.to_string(),
                truncate_message(&entry.query, query_width),
                entry.duration.clone(),
                entry.ago.clone(),
            ])
        })
        .collect();

    let widths = [
        Constraint::Length(6),  // Index
        Constraint::Min(20),    // Query
        Constraint::Length(11), // Time
        Constraint::Length(13), // Ago
    ];

    let table = Table::new(rows, widths)
        .header(header)
        .row_highlight_style(common_styles::SELECTED_ROW_STYLE)
        .highlight_symbol(">> ")
        .highlight_spacing(HighlightSpacing::Always);

    frame.render_stateful_widget(table, inner_area, table_state);
}
