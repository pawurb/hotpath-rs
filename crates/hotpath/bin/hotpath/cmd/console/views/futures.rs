pub(crate) mod calls;
pub(crate) mod inspect;

use super::common_styles;
use crate::cmd::console::app::FuturesFocus;
use crate::cmd::console::widgets::formatters::truncate_left;
use hotpath::json::JsonFutureEntry;
use ratatui::{
    layout::{Constraint, Rect},
    style::Style,
    symbols::border,
    widgets::{Block, Cell, HighlightSpacing, Row, Table, TableState},
    Frame,
};

/// Renders the futures table with future statistics
#[hotpath::measure]
#[allow(clippy::too_many_arguments)]
pub(crate) fn render_futures_panel(
    stats: &[JsonFutureEntry],
    area: Rect,
    frame: &mut Frame,
    table_state: &mut TableState,
    show_calls: bool,
    focus: FuturesFocus,
    future_position: usize,
    total_futures: usize,
) {
    let available_width = area.width.saturating_sub(10);
    let future_width = ((available_width as f32 * 0.50) as usize).max(30);

    let header = Row::new(vec![
        Cell::from("Future"),
        Cell::from("Calls"),
        Cell::from("Polls"),
    ])
    .style(common_styles::HEADER_STYLE)
    .height(1);

    let rows: Vec<Row> = stats
        .iter()
        .map(|stat| {
            Row::new(vec![
                Cell::from(truncate_left(&stat.label, future_width)),
                Cell::from(stat.call_count.to_string()),
                Cell::from(stat.total_polls.to_string()),
            ])
        })
        .collect();

    let widths = [
        Constraint::Percentage(50), // Future
        Constraint::Percentage(25), // Calls
        Constraint::Percentage(25), // Polls
    ];

    let table_block = if show_calls {
        let border_set = if focus == FuturesFocus::Futures {
            border::THICK
        } else {
            border::PLAIN
        };
        Block::bordered()
            .title(format!(" [{}/{}] ", future_position, total_futures))
            .border_set(border_set)
            .border_style(if focus == FuturesFocus::Futures {
                Style::default()
            } else {
                common_styles::UNFOCUSED_BORDER_STYLE
            })
    } else {
        Block::bordered()
            .title(format!(" [{}/{}] ", future_position, total_futures))
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
