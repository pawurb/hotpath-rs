use crate::cmd::console::app::InspectedDataFlowLog;
use hotpath::format_bytes;
use ratatui::{
    layout::Rect,
    symbols::border,
    text::Line,
    widgets::{Block, Clear, Paragraph, Wrap},
    Frame,
};

fn wrap_text(text: &str, max_width: usize) -> Vec<Line<'static>> {
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

pub(crate) fn render_inspect_popup(
    inspected: &InspectedDataFlowLog,
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

    let (title, message) = match inspected {
        InspectedDataFlowLog::ChannelSent(entry) => {
            let title = format!(" Message (Index: {}) - {} ", entry.index, entry.timestamp);
            let message = entry
                .message
                .as_deref()
                .unwrap_or("(missing \"log = true\")");
            (title, message.to_string())
        }
        InspectedDataFlowLog::Stream(entry) => {
            let title = format!(" Message (Index: {}) - {} ", entry.index, entry.timestamp);
            let message = entry
                .message
                .as_deref()
                .unwrap_or("(missing \"log = true\")");
            (title, message.to_string())
        }
        InspectedDataFlowLog::FutureCall(call) => {
            let total_alloc = call
                .total_poll_alloc_bytes
                .map(format_bytes)
                .unwrap_or_else(|| "-".to_string());
            let title = format!(
                " Result (Call ID: {}, State: {}, Alloc: {}) ",
                call.id, call.state, total_alloc
            );
            let message = call.result.as_deref().unwrap_or("(no result available)");
            (title, message.to_string())
        }
    };

    let block = Block::bordered().title(title).border_set(border::DOUBLE);

    let inner_area = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    let max_width = inner_area.width.saturating_sub(2) as usize;
    let text_lines = wrap_text(&message, max_width);

    let paragraph = Paragraph::new(text_lines).wrap(Wrap { trim: false });
    frame.render_widget(paragraph, inner_area);
}
