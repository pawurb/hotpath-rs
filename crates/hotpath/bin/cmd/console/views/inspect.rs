use crate::cmd::console::widgets::formatters::format_timestamp;
use hotpath::channels::LogEntry;
use ratatui::{
    layout::Rect,
    symbols::border,
    text::Line,
    widgets::{Block, Clear},
    Frame,
};

/// Renders a centered popup displaying the full log message
pub(crate) fn render_inspect_popup(entry: &LogEntry, area: Rect, frame: &mut Frame) {
    // Center the popup at 80% of screen size
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

    let message = entry
        .message
        .as_deref()
        .unwrap_or("(missing \"log = true\")");

    frame.render_widget(Clear, popup_area);

    let block = Block::bordered()
        .title(format!(
            " Message (Index: {}) - {} ",
            entry.index,
            format_timestamp(entry.timestamp)
        ))
        .border_set(border::DOUBLE);

    let inner_area = block.inner(popup_area);

    frame.render_widget(block, popup_area);

    let text_lines: Vec<Line> = message
        .lines()
        .flat_map(|line| {
            let max_width = inner_area.width.saturating_sub(2) as usize;
            if line.len() <= max_width {
                vec![Line::from(line)]
            } else {
                let mut wrapped = Vec::new();
                let mut remaining = line;
                while !remaining.is_empty() {
                    let split_at = remaining
                        .char_indices()
                        .nth(max_width)
                        .map(|(i, _)| i)
                        .unwrap_or(remaining.len());
                    wrapped.push(Line::from(&remaining[..split_at]));
                    remaining = &remaining[split_at..];
                }
                wrapped
            }
        })
        .collect();

    let paragraph =
        ratatui::widgets::Paragraph::new(text_lines).wrap(ratatui::widgets::Wrap { trim: false });

    frame.render_widget(paragraph, inner_area);
}
