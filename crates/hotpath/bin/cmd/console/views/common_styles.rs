use ratatui::style::{Color, Modifier, Style};

pub const HEADER_STYLE: Style = Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD);
pub const HEADER_STYLE_CYAN: Style = Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD);
pub const SELECTED_ROW_STYLE: Style = Style::new()
    .bg(Color::DarkGray)
    .add_modifier(Modifier::BOLD);
pub const TITLE_STYLE_YELLOW: Style = Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD);
pub const UNFOCUSED_BORDER_STYLE: Style = Style::new().fg(Color::DarkGray);
pub const PLACEHOLDER_STYLE: Style = Style::new().fg(Color::DarkGray);
