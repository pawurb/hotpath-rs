use crate::cmd::console::app::{Focus, SelectedTab};
use ratatui::{
    layout::Rect,
    style::Stylize,
    symbols::border,
    text::Line,
    widgets::{Block, Paragraph},
    Frame,
};

/// Renders the bottom controls bar showing context-aware keybindings
pub(crate) fn render_help_bar(
    frame: &mut Frame,
    area: Rect,
    selected_tab: SelectedTab,
    focus: Focus,
) {
    let controls_line = if selected_tab == SelectedTab::Channels {
        match focus {
            Focus::Channels => Line::from(vec![
                " Quit ".into(),
                "<q> ".blue().bold(),
                " | Navigate ".into(),
                "<←↑↓→/hjkl> ".blue().bold(),
                " | Toggle Logs ".into(),
                "<o> ".blue().bold(),
                " | Pause ".into(),
                "<p> ".blue().bold(),
            ]),
            Focus::Logs => Line::from(vec![
                " Quit ".into(),
                "<q> ".blue().bold(),
                " | Navigate ".into(),
                "<←↑↓→/hjkl> ".blue().bold(),
                " | Toggle Logs ".into(),
                "<o> ".blue().bold(),
                " | Pause ".into(),
                "<p> ".blue().bold(),
                " | Inspect ".into(),
                "<i> ".blue().bold(),
            ]),
            Focus::Inspect => Line::from(vec![
                " Quit ".into(),
                "<q> ".blue().bold(),
                " | Navigate ".into(),
                "<←↑↓→/hjkl> ".blue().bold(),
                " | Toggle Logs ".into(),
                "<o> ".blue().bold(),
                " | Pause ".into(),
                "<p> ".blue().bold(),
                " | Close ".into(),
                "<i/o/h> ".blue().bold(),
            ]),
        }
    } else {
        Line::from(vec![
            " Tabs ".into(),
            "<1/2> ".blue().bold(),
            " | Navigate ".into(),
            "<↑/k ↓/j> ".blue().bold(),
            " | Toggle Samples ".into(),
            "<o> ".blue().bold(),
            " | Pause ".into(),
            "<p> ".blue().bold(),
            " | Quit ".into(),
            "<q> ".blue().bold(),
        ])
    };

    let block = Block::bordered()
        .title(" Controls ")
        .border_set(border::PLAIN);

    let paragraph = Paragraph::new(controls_line).block(block).left_aligned();

    frame.render_widget(paragraph, area);
}
