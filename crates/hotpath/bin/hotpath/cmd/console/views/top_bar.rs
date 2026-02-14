use ratatui::{
    layout::Rect,
    style::Stylize,
    symbols::border,
    text::{Line, Span},
    widgets::{Block, Paragraph},
    Frame,
};
use std::time::Instant;

#[hotpath::measure]
pub(crate) fn render_status_bar(
    frame: &mut Frame,
    area: Rect,
    is_paused: bool,
    last_successful_fetch: Option<Instant>,
    has_error: bool,
    has_data: bool,
    program_uptime: Option<&str>,
) {
    let uptime_spans: Vec<Span> = match program_uptime {
        Some(uptime) if !uptime.is_empty() => {
            vec![" | ".dark_gray(), "Uptime: ".into(), uptime.cyan().bold()]
        }
        _ => vec![],
    };

    let status_text = if is_paused {
        let mut spans: Vec<Span> = vec!["⏸ ".yellow(), "PAUSED".yellow().bold()];
        spans.extend(uptime_spans);
        Line::from(spans)
    } else if let Some(last_fetch) = last_successful_fetch {
        let elapsed = Instant::now().duration_since(last_fetch);
        let seconds = elapsed.as_secs();

        let is_stale = has_error && has_data && seconds > 3;

        if is_stale {
            let mut spans: Vec<Span> = vec![
                "⚠ ".yellow(),
                "Stale ".into(),
                format!("(refreshed {}s ago)", seconds).yellow(),
            ];
            spans.extend(uptime_spans);
            Line::from(spans)
        } else {
            let mut spans: Vec<Span> = vec![
                "✓ ".green(),
                "Live ".green().bold(),
                format!("(refreshed {}s ago)", seconds).into(),
            ];
            spans.extend(uptime_spans);
            Line::from(spans)
        }
    } else {
        Line::from(vec!["⋯ ".into(), "Connecting...".into()])
    };

    let block = Block::bordered()
        .title(" Status ")
        .border_set(border::PLAIN);

    let paragraph = Paragraph::new(status_text).block(block).left_aligned();

    frame.render_widget(paragraph, area);
}
