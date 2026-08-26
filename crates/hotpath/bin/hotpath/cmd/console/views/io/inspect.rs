use hotpath::json::{JsonHttpLog, JsonSqlLog};
use ratatui::{
    layout::{Constraint, Layout, Rect},
    symbols::border,
    text::Line,
    widgets::{Block, Clear, Paragraph, Wrap},
    Frame,
};

fn wrap_text(text: &str, max_width: usize) -> Vec<Line<'static>> {
    // A zero width would make the split loop below yield empty slices forever.
    let max_width = max_width.max(1);
    text.lines()
        .flat_map(|line| {
            if line.len() <= max_width {
                vec![Line::from(line.to_string())]
            } else {
                let mut wrapped = Vec::new();
                let mut remaining = line;
                while !remaining.is_empty() {
                    let split_at = remaining
                        .char_indices()
                        .nth(max_width)
                        .map(|(i, _)| i)
                        .unwrap_or(remaining.len());
                    wrapped.push(Line::from(remaining[..split_at].to_string()));
                    remaining = &remaining[split_at..];
                }
                wrapped
            }
        })
        .collect()
}

fn header_lines(
    details_text: &str,
    source: Option<&str>,
    route: Option<&str>,
    max_width: usize,
) -> Vec<Line<'static>> {
    let mut lines = wrap_text(details_text, max_width);
    if let Some(source) = source {
        lines.extend(wrap_text(&format!("Source: {}", source), max_width));
    }
    if let Some(route) = route {
        lines.extend(wrap_text(&format!("Route: {}", route), max_width));
    }
    lines
}

pub(crate) fn render_sql_inspect_popup(
    log: &JsonSqlLog,
    source: Option<&str>,
    route: Option<&str>,
    area: Rect,
    frame: &mut Frame,
) {
    let popup_width = (area.width as f32 * 0.8) as u16;
    let popup_height = (area.height as f32 * 0.8) as u16;
    let x = (area.width.saturating_sub(popup_width)) / 2;
    let y = (area.height.saturating_sub(popup_height)) / 2;

    let popup_area = Rect {
        x: area.x + x,
        y: area.y + y,
        width: popup_width,
        height: popup_height,
    };

    frame.render_widget(Clear, popup_area);

    let block = Block::bordered()
        .title(format!(" Query execution #{} ", log.index))
        .border_set(border::DOUBLE);

    let inner_area = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    let max_width = inner_area.width.saturating_sub(2) as usize;
    let details_text = format!("Time: {} | Executed: {}", log.duration, log.ago);
    let details_lines = header_lines(&details_text, source, route, max_width);

    let [details_area, _, query_area] = Layout::vertical([
        Constraint::Length(details_lines.len() as u16),
        Constraint::Length(1),
        Constraint::Min(0),
    ])
    .areas(inner_area);

    let details = Paragraph::new(details_lines).wrap(Wrap { trim: false });
    frame.render_widget(details, details_area);

    let query_lines = wrap_text(&log.query, max_width);
    let query = Paragraph::new(query_lines).wrap(Wrap { trim: false });
    frame.render_widget(query, query_area);
}

pub(crate) fn render_http_inspect_popup(
    log: &JsonHttpLog,
    source: Option<&str>,
    route: Option<&str>,
    area: Rect,
    frame: &mut Frame,
) {
    let popup_width = (area.width as f32 * 0.8) as u16;
    let popup_height = (area.height as f32 * 0.8) as u16;
    let x = (area.width.saturating_sub(popup_width)) / 2;
    let y = (area.height.saturating_sub(popup_height)) / 2;

    let popup_area = Rect {
        x: area.x + x,
        y: area.y + y,
        width: popup_width,
        height: popup_height,
    };

    frame.render_widget(Clear, popup_area);

    let block = Block::bordered()
        .title(format!(" Request execution #{} ", log.index))
        .border_set(border::DOUBLE);

    let inner_area = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    let max_width = inner_area.width.saturating_sub(2) as usize;
    let details_text = format!("Time: {} | Executed: {}", log.duration, log.ago);
    let details_lines = header_lines(&details_text, source, route, max_width);

    let [details_area, _, status_area] = Layout::vertical([
        Constraint::Length(details_lines.len() as u16),
        Constraint::Length(1),
        Constraint::Min(0),
    ])
    .areas(inner_area);

    let details = Paragraph::new(details_lines).wrap(Wrap { trim: false });
    frame.render_widget(details, details_area);

    let status_lines = wrap_text(&format!("Status: {}", log.status), max_width);
    let status = Paragraph::new(status_lines).wrap(Wrap { trim: false });
    frame.render_widget(status, status_area);
}
