use ratatui::{
    layout::Rect,
    style::Stylize,
    symbols::border,
    text::Line,
    widgets::{Block, Paragraph},
    Frame,
};
use std::time::Instant;

/// Renders the top status bar showing connection status and refresh timer
pub(crate) fn render_status_bar(
    frame: &mut Frame,
    area: Rect,
    is_paused: bool,
    last_successful_fetch: Option<Instant>,
    has_error: bool,
    has_data: bool,
) {
    let status_text = if is_paused {
        Line::from(vec!["⏸ ".yellow(), "PAUSED".yellow().bold()])
    } else if let Some(last_fetch) = last_successful_fetch {
        let elapsed = Instant::now().duration_since(last_fetch);
        let seconds = elapsed.as_secs();

        let is_stale = has_error && has_data;

        if is_stale {
            Line::from(vec![
                "⚠ ".yellow(),
                "Stale ".into(),
                format!("(refreshed {}s ago)", seconds).yellow(),
            ])
        } else {
            Line::from(vec![
                "✓ ".green(),
                "Live ".green().bold(),
                format!("(refreshed {}s ago)", seconds).into(),
            ])
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
