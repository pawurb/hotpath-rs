use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

pub(crate) fn render_help_bar(frame: &mut Frame, area: Rect) {
    let spans = vec![
        Span::raw("Tabs "),
        Span::styled(
            "<1/2>",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" | Navigate "),
        Span::styled(
            "<↑/k ↓/j>",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" | Toggle Samples "),
        Span::styled(
            "<o>",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" | Pause "),
        Span::styled(
            "<p>",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" | Quit "),
        Span::styled(
            "<q>",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
    ];

    let help_text = vec![Line::from(spans)];

    let help_paragraph =
        Paragraph::new(help_text).block(Block::default().borders(Borders::ALL).title("Controls"));

    frame.render_widget(help_paragraph, area);
}
