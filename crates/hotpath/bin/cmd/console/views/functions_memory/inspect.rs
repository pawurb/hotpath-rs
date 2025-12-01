use super::super::super::app::InspectedFunctionLog;
use ratatui::{
    layout::Rect,
    symbols::border,
    text::Line,
    widgets::{Block, Clear, Paragraph, Wrap},
    Frame,
};

/// Renders a centered popup displaying the full result value for a function log entry (memory mode)
pub(crate) fn render_inspect_popup(
    entry: &InspectedFunctionLog,
    area: Rect,
    frame: &mut Frame,
    total_elapsed: u64,
) {
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

    let result_text = entry
        .result
        .as_deref()
        .unwrap_or("(no result - missing \"log = true\")");

    frame.render_widget(Clear, popup_area);

    let mem_str = entry
        .value
        .map_or("N/A".to_string(), |v| hotpath::format_bytes(v));
    let obj_str = entry
        .alloc_count
        .map_or("N/A".to_string(), |c| c.to_string());
    let time_ago_str = if total_elapsed >= entry.elapsed_nanos {
        let nanos_ago = total_elapsed - entry.elapsed_nanos;
        super::super::super::widgets::formatters::format_time_ago(nanos_ago)
    } else {
        "now".to_string()
    };
    let tid_str = entry.tid.map_or("N/A".to_string(), |t| t.to_string());

    let block = Block::bordered()
        .title(format!(
            " Result (Call #{}, Mem: {}, Objects: {}, Ago: {}, TID: {}) ",
            entry.invocation_index, mem_str, obj_str, time_ago_str, tid_str
        ))
        .border_set(border::DOUBLE);

    let inner_area = block.inner(popup_area);

    frame.render_widget(block, popup_area);

    let text_lines: Vec<Line> = result_text
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

    let paragraph = Paragraph::new(text_lines).wrap(Wrap { trim: false });

    frame.render_widget(paragraph, inner_area);
}
