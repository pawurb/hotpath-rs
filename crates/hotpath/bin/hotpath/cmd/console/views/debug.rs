pub(crate) mod inspect;
pub(crate) mod logs;

use crate::cmd::console::app::DebugFocus;
use crate::cmd::console::views::common_styles;
use crate::cmd::console::widgets::formatters::truncate_left;
use hotpath::json::FormattedDbgStats;
use ratatui::{
    layout::{Constraint, Rect},
    style::Style,
    symbols::border,
    widgets::{Block, Cell, HighlightSpacing, Row, Table, TableState},
    Frame,
};

#[hotpath::measure]
#[allow(clippy::too_many_arguments)]
pub(crate) fn render_debug_panel(
    stats: &[FormattedDbgStats],
    area: Rect,
    frame: &mut Frame,
    table_state: &mut TableState,
    show_logs: bool,
    focus: DebugFocus,
    position: usize,
    total: usize,
) {
    let available_width = area.width.saturating_sub(10);
    let source_width = ((available_width as f32 * 0.25) as usize).max(15);
    let expr_width = ((available_width as f32 * 0.25) as usize).max(15);
    let value_width = ((available_width as f32 * 0.35) as usize).max(20);

    let header = Row::new(vec![
        Cell::from("Source"),
        Cell::from("Expression"),
        Cell::from("Last Value"),
        Cell::from("Count"),
    ])
    .style(common_styles::HEADER_STYLE)
    .height(1);

    let rows: Vec<Row> = stats
        .iter()
        .map(|stat| {
            let last_value = stat.last_value.as_deref().unwrap_or("-");
            Row::new(vec![
                Cell::from(truncate_left(&stat.source_display, source_width)),
                Cell::from(truncate_left(&stat.expression, expr_width)),
                Cell::from(truncate_left(last_value, value_width)),
                Cell::from(stat.log_count.to_string()),
            ])
        })
        .collect();

    let widths = [
        Constraint::Percentage(25),
        Constraint::Percentage(25),
        Constraint::Percentage(35),
        Constraint::Percentage(15),
    ];

    let table_block = if show_logs {
        let border_set = if focus == DebugFocus::Debug {
            border::THICK
        } else {
            border::PLAIN
        };
        Block::bordered()
            .title(format!(" [{}/{}] ", position, total))
            .border_set(border_set)
            .border_style(if focus == DebugFocus::Debug {
                Style::default()
            } else {
                common_styles::UNFOCUSED_BORDER_STYLE
            })
    } else {
        Block::bordered()
            .title(format!(" [{}/{}] ", position, total))
            .border_set(border::THICK)
    };

    let table = Table::new(rows, widths)
        .header(header)
        .block(table_block)
        .column_spacing(1)
        .row_highlight_style(common_styles::SELECTED_ROW_STYLE)
        .highlight_symbol(">> ")
        .highlight_spacing(HighlightSpacing::Always);

    frame.render_stateful_widget(table, area, table_state);
}
