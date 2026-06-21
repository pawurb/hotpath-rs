pub(crate) mod inspect;
pub(crate) mod logs;

use crate::cmd::console::app::DataFlowFocus;
use crate::cmd::console::views::common_styles;
use crate::cmd::console::widgets::formatters::truncate_left;
use hotpath::json::{
    JsonChannelEntry, JsonFutureEntry, JsonMutexEntry, JsonRwLockEntry, JsonStreamEntry,
};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    symbols::border,
    text::Span,
    widgets::{Block, Cell, HighlightSpacing, Row, Table, TableState},
    Frame,
};

fn channel_capacity(channel_type: &str) -> String {
    match channel_type {
        "unbounded" => "∞".to_string(),
        "oneshot" => "1".to_string(),
        s if s.starts_with("bounded") => {
            if let Some(start) = s.find('[') {
                if let Some(end) = s.find(']') {
                    s[start + 1..end].to_string()
                } else {
                    s[start + 1..].to_string()
                }
            } else {
                "?".to_string()
            }
        }
        _ => "?".to_string(),
    }
}

fn state_style(state: &str) -> Style {
    match state {
        "active" | "Active" => Style::default().fg(Color::Green),
        "closed" | "Closed" => Style::default().fg(Color::Yellow),
        "Ready" => Style::default().fg(Color::Green),
        "Cancelled" => Style::default().fg(Color::Red),
        "Suspended" => Style::default().fg(Color::Yellow),
        "Running" => Style::default().fg(Color::Blue),
        "Pending" => Style::default().fg(Color::DarkGray),
        _ => Style::default(),
    }
}

fn list_block(
    title: &'static str,
    show_logs: bool,
    focus: DataFlowFocus,
    position: usize,
    total: usize,
) -> Block<'static> {
    if show_logs {
        let border_set = if focus == DataFlowFocus::List {
            border::THICK
        } else {
            border::PLAIN
        };
        Block::bordered()
            .title(format!(" [{}/{}] ", position, total))
            .title(Span::styled(title, common_styles::TITLE_STYLE_YELLOW))
            .border_set(border_set)
            .border_style(if focus == DataFlowFocus::List {
                Style::default()
            } else {
                common_styles::UNFOCUSED_BORDER_STYLE
            })
    } else {
        Block::bordered()
            .title(format!(" [{}/{}] ", position, total))
            .title(Span::styled(title, common_styles::TITLE_STYLE_YELLOW))
            .border_set(border::THICK)
    }
}

#[hotpath::measure]
#[allow(clippy::too_many_arguments)]
pub(crate) fn render_channels_panel(
    entries: &[JsonChannelEntry],
    percentiles: &[f64],
    area: Rect,
    frame: &mut Frame,
    table_state: &mut TableState,
    show_logs: bool,
    focus: DataFlowFocus,
    position: usize,
    total: usize,
) {
    let available_width = area.width.saturating_sub(10);
    let label_width = ((available_width as f32 * 0.30) as usize).max(20);

    let header_cells = vec![
        "Type".to_string(),
        "Label".to_string(),
        "State".to_string(),
        "Sent/Recv".to_string(),
        "Rate s/r".to_string(),
        "Queue/Max/Cap".to_string(),
        "Avg".to_string(),
    ]
    .into_iter()
    .chain(
        percentiles
            .iter()
            .map(|p| hotpath::format_percentile_header(*p)),
    )
    .map(Cell::from)
    .collect::<Vec<_>>();

    let header = Row::new(header_cells)
        .style(common_styles::HEADER_STYLE_CYAN)
        .height(1);

    let percentile_keys: Vec<String> = percentiles
        .iter()
        .map(|p| hotpath::format_percentile_key(*p))
        .collect();

    let rows: Vec<Row> = entries
        .iter()
        .map(|entry| {
            let capacity = channel_capacity(&entry.channel_type);
            let type_text = format!("Channel[{capacity}]");
            // Queue depth is only tracked for wrap channels; proxy channels show `-`.
            let queue_text = match (entry.queue_size, entry.max_queue_size) {
                (Some(queue), Some(max)) => format!("{queue}/{max}/{capacity}"),
                _ => "-".to_string(),
            };
            let rate_text = format!(
                "{}/{}",
                hotpath::format_rate(entry.sent_per_sec),
                hotpath::format_rate(entry.received_per_sec)
            );
            // Latency is only measured for wrap channels; proxy channels show `-`.
            let mut cells = vec![
                Cell::from(type_text).style(Style::default().fg(Color::Cyan)),
                Cell::from(truncate_left(&entry.label, label_width)),
                Cell::from(entry.state.clone()).style(state_style(&entry.state)),
                Cell::from(format!("{}/{}", entry.sent_count, entry.received_count)),
                Cell::from(rate_text),
                Cell::from(queue_text),
                Cell::from(entry.proc_avg.clone().unwrap_or_else(|| "-".to_string())),
            ];
            for key in &percentile_keys {
                let value = entry
                    .proc_percentiles
                    .get(key)
                    .cloned()
                    .unwrap_or_else(|| "-".to_string());
                cells.push(Cell::from(value));
            }
            Row::new(cells)
        })
        .collect();

    let widths = vec![
        Constraint::Length(12),
        Constraint::Percentage(30),
        Constraint::Length(10),
        Constraint::Length(14),
        Constraint::Length(15),
        Constraint::Length(15),
        Constraint::Length(12),
    ]
    .into_iter()
    .chain((0..percentile_keys.len()).map(|_| Constraint::Length(12)))
    .collect::<Vec<_>>();

    let table = Table::new(rows, widths)
        .header(header)
        .block(list_block(
            " Channels - message flow ",
            show_logs,
            focus,
            position,
            total,
        ))
        .column_spacing(1)
        .row_highlight_style(common_styles::SELECTED_ROW_STYLE)
        .highlight_symbol(">> ")
        .highlight_spacing(HighlightSpacing::Always);

    frame.render_stateful_widget(table, area, table_state);
}

#[hotpath::measure]
#[allow(clippy::too_many_arguments)]
pub(crate) fn render_streams_panel(
    entries: &[JsonStreamEntry],
    area: Rect,
    frame: &mut Frame,
    table_state: &mut TableState,
    show_logs: bool,
    focus: DataFlowFocus,
    position: usize,
    total: usize,
) {
    let available_width = area.width.saturating_sub(10);
    let label_width = ((available_width as f32 * 0.6) as usize).max(20);

    let header = Row::new(vec![
        Cell::from("Label"),
        Cell::from("State"),
        Cell::from("Yielded"),
    ])
    .style(common_styles::HEADER_STYLE_CYAN)
    .height(1);

    let rows: Vec<Row> = entries
        .iter()
        .map(|entry| {
            Row::new(vec![
                Cell::from(truncate_left(&entry.label, label_width)),
                Cell::from(entry.state.clone()).style(state_style(&entry.state)),
                Cell::from(entry.items_yielded.to_string()),
            ])
        })
        .collect();

    let widths = [
        Constraint::Percentage(70),
        Constraint::Length(10),
        Constraint::Length(12),
    ];

    let table = Table::new(rows, widths)
        .header(header)
        .block(list_block(
            " Streams - items yielded ",
            show_logs,
            focus,
            position,
            total,
        ))
        .column_spacing(1)
        .row_highlight_style(common_styles::SELECTED_ROW_STYLE)
        .highlight_symbol(">> ")
        .highlight_spacing(HighlightSpacing::Always);

    frame.render_stateful_widget(table, area, table_state);
}

#[hotpath::measure]
#[allow(clippy::too_many_arguments)]
pub(crate) fn render_futures_panel(
    entries: &[JsonFutureEntry],
    area: Rect,
    frame: &mut Frame,
    table_state: &mut TableState,
    show_logs: bool,
    focus: DataFlowFocus,
    position: usize,
    total: usize,
) {
    let available_width = area.width.saturating_sub(10);
    let label_width = ((available_width as f32 * 0.55) as usize).max(20);

    let header = Row::new(vec![
        Cell::from("Label"),
        Cell::from("Calls"),
        Cell::from("Polls"),
        Cell::from("Avg Total"),
    ])
    .style(common_styles::HEADER_STYLE_CYAN)
    .height(1);

    let rows: Vec<Row> = entries
        .iter()
        .map(|entry| {
            let display_label = hotpath::shorten_function_name(&entry.label);
            let avg_total_ns = entry
                .total_poll_duration_ns
                .checked_div(entry.call_count)
                .unwrap_or(0);
            Row::new(vec![
                Cell::from(truncate_left(&display_label, label_width)),
                Cell::from(entry.call_count.to_string()),
                Cell::from(entry.total_polls.to_string()),
                Cell::from(hotpath::format_duration(avg_total_ns)),
            ])
        })
        .collect();

    let widths = [
        Constraint::Percentage(55),
        Constraint::Length(10),
        Constraint::Length(10),
        Constraint::Length(12),
    ];

    let table = Table::new(rows, widths)
        .header(header)
        .block(list_block(
            " Futures - poll lifecycle ",
            show_logs,
            focus,
            position,
            total,
        ))
        .column_spacing(1)
        .row_highlight_style(common_styles::SELECTED_ROW_STYLE)
        .highlight_symbol(">> ")
        .highlight_spacing(HighlightSpacing::Always);

    frame.render_stateful_widget(table, area, table_state);
}

/// Whether to read a lock's read or write columns when building a sub-table.
#[derive(Clone, Copy)]
enum RwKind {
    Read,
    Write,
}

#[hotpath::measure]
#[allow(clippy::too_many_arguments)]
pub(crate) fn render_rw_locks_panel(
    entries: &[JsonRwLockEntry],
    percentiles: &[f64],
    area: Rect,
    frame: &mut Frame,
    table_state: &mut TableState,
    position: usize,
    total: usize,
) {
    // Stack a reads table over a writes table; both list every lock in the same
    // order so the shared cursor highlights the same lock in both halves.
    let halves = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    render_rw_locks_subtable(
        entries,
        percentiles,
        RwKind::Read,
        halves[0],
        frame,
        table_state,
        position,
        total,
    );
    render_rw_locks_subtable(
        entries,
        percentiles,
        RwKind::Write,
        halves[1],
        frame,
        table_state,
        position,
        total,
    );
}

#[allow(clippy::too_many_arguments)]
fn render_rw_locks_subtable(
    entries: &[JsonRwLockEntry],
    percentiles: &[f64],
    kind: RwKind,
    area: Rect,
    frame: &mut Frame,
    table_state: &mut TableState,
    position: usize,
    total: usize,
) {
    let available_width = area.width.saturating_sub(10);
    let label_width = ((available_width as f32 * 0.30) as usize).max(16);

    let percentile_keys: Vec<String> = percentiles
        .iter()
        .map(|p| hotpath::format_percentile_key(*p))
        .collect();

    let (count_label, title) = match kind {
        RwKind::Read => ("Reads", " RwLocks reads - wait & acquire time "),
        RwKind::Write => ("Writes", " RwLocks writes - wait & acquire time "),
    };

    let mut header_cells = vec![
        Cell::from("Lock"),
        Cell::from(count_label),
        Cell::from("Wait avg"),
    ];
    for p in percentiles {
        header_cells.push(Cell::from(format!(
            "Wait {}",
            hotpath::format_percentile_header(*p)
        )));
    }
    header_cells.push(Cell::from("Acq avg"));
    for p in percentiles {
        header_cells.push(Cell::from(format!(
            "Acq {}",
            hotpath::format_percentile_header(*p)
        )));
    }
    let header = Row::new(header_cells)
        .style(common_styles::HEADER_STYLE_CYAN)
        .height(1);

    let rows: Vec<Row> = entries
        .iter()
        .map(|entry| {
            let (count, wait_avg, wait_percentiles, acq_avg, acq_percentiles) = match kind {
                RwKind::Read => (
                    entry.read_count,
                    &entry.read_wait_avg,
                    &entry.read_wait_percentiles,
                    &entry.read_acquire_avg,
                    &entry.read_acquire_percentiles,
                ),
                RwKind::Write => (
                    entry.write_count,
                    &entry.write_wait_avg,
                    &entry.write_wait_percentiles,
                    &entry.write_acquire_avg,
                    &entry.write_acquire_percentiles,
                ),
            };

            let mut cells = vec![
                Cell::from(truncate_left(&entry.label, label_width)),
                Cell::from(count.to_string()),
                Cell::from(wait_avg.clone()),
            ];
            for key in &percentile_keys {
                cells.push(Cell::from(
                    wait_percentiles.get(key).cloned().unwrap_or_default(),
                ));
            }
            cells.push(Cell::from(acq_avg.clone()));
            for key in &percentile_keys {
                cells.push(Cell::from(
                    acq_percentiles.get(key).cloned().unwrap_or_default(),
                ));
            }
            Row::new(cells)
        })
        .collect();

    let mut widths = vec![
        Constraint::Percentage(30),
        Constraint::Length(8),
        Constraint::Length(10),
    ];
    for _ in percentiles {
        widths.push(Constraint::Length(10));
    }
    widths.push(Constraint::Length(10));
    for _ in percentiles {
        widths.push(Constraint::Length(10));
    }

    let table = Table::new(rows, widths)
        .header(header)
        .block(list_block(
            title,
            false,
            DataFlowFocus::List,
            position,
            total,
        ))
        .column_spacing(1)
        .row_highlight_style(common_styles::SELECTED_ROW_STYLE)
        .highlight_symbol(">> ")
        .highlight_spacing(HighlightSpacing::Always);

    frame.render_stateful_widget(table, area, table_state);
}

#[hotpath::measure]
#[allow(clippy::too_many_arguments)]
pub(crate) fn render_mutexes_panel(
    entries: &[JsonMutexEntry],
    percentiles: &[f64],
    area: Rect,
    frame: &mut Frame,
    table_state: &mut TableState,
    position: usize,
    total: usize,
) {
    let available_width = area.width.saturating_sub(10);
    let label_width = ((available_width as f32 * 0.30) as usize).max(16);

    let percentile_keys: Vec<String> = percentiles
        .iter()
        .map(|p| hotpath::format_percentile_key(*p))
        .collect();

    let mut header_cells = vec![
        Cell::from("Mutex"),
        Cell::from("Locks"),
        Cell::from("Wait avg"),
    ];
    for p in percentiles {
        header_cells.push(Cell::from(format!(
            "Wait {}",
            hotpath::format_percentile_header(*p)
        )));
    }
    header_cells.push(Cell::from("Acq avg"));
    for p in percentiles {
        header_cells.push(Cell::from(format!(
            "Acq {}",
            hotpath::format_percentile_header(*p)
        )));
    }
    let header = Row::new(header_cells)
        .style(common_styles::HEADER_STYLE_CYAN)
        .height(1);

    let rows: Vec<Row> = entries
        .iter()
        .map(|entry| {
            let mut cells = vec![
                Cell::from(truncate_left(&entry.label, label_width)),
                Cell::from(entry.count.to_string()),
                Cell::from(entry.wait_avg.clone()),
            ];
            for key in &percentile_keys {
                cells.push(Cell::from(
                    entry.wait_percentiles.get(key).cloned().unwrap_or_default(),
                ));
            }
            cells.push(Cell::from(entry.acquire_avg.clone()));
            for key in &percentile_keys {
                cells.push(Cell::from(
                    entry
                        .acquire_percentiles
                        .get(key)
                        .cloned()
                        .unwrap_or_default(),
                ));
            }
            Row::new(cells)
        })
        .collect();

    let mut widths = vec![
        Constraint::Percentage(30),
        Constraint::Length(8),
        Constraint::Length(10),
    ];
    for _ in percentiles {
        widths.push(Constraint::Length(10));
    }
    widths.push(Constraint::Length(10));
    for _ in percentiles {
        widths.push(Constraint::Length(10));
    }

    let table = Table::new(rows, widths)
        .header(header)
        .block(list_block(
            " Mutexes - wait & acquire time ",
            false,
            DataFlowFocus::List,
            position,
            total,
        ))
        .column_spacing(1)
        .row_highlight_style(common_styles::SELECTED_ROW_STYLE)
        .highlight_symbol(">> ")
        .highlight_spacing(HighlightSpacing::Always);

    frame.render_stateful_widget(table, area, table_state);
}
