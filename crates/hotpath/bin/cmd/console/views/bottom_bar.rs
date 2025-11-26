use crate::cmd::console::app::{ChannelsFocus, FunctionsFocus, SelectedTab, StreamsFocus};
use ratatui::{
    layout::Rect,
    style::Stylize,
    symbols::border,
    text::Line,
    widgets::{Block, Paragraph},
    Frame,
};

// Control text constants
const NAV_KEYS_FULL: &str = " <←↑↓→/hjkl> ";
const TOGGLE_LOGS_LABEL: &str = " | Toggle Logs ";
const TOGGLE_LOGS_KEY: &str = "<o> ";
const PAUSE_LABEL: &str = " | Pause ";
const PAUSE_KEY: &str = "<p> ";
const QUIT_LABEL: &str = " | Quit ";
const QUIT_KEY: &str = "<q> ";
const INSPECT_LABEL: &str = " | Inspect ";
const INSPECT_KEY: &str = "<i> ";
const CLOSE_LABEL: &str = " | Close ";
const CLOSE_KEYS: &str = "<i/o/h> ";

/// Renders the bottom controls bar showing context-aware keybindings
#[cfg_attr(feature = "hotpath", hotpath::measure)]
pub(crate) fn render_help_bar(
    frame: &mut Frame,
    area: Rect,
    selected_tab: SelectedTab,
    channels_focus: ChannelsFocus,
    streams_focus: StreamsFocus,
    functions_focus: FunctionsFocus,
) {
    let controls_line = if selected_tab == SelectedTab::Threads {
        // Threads tab - simple controls, no logs
        Line::from(vec![
            NAV_KEYS_FULL.blue().bold(),
            PAUSE_LABEL.into(),
            PAUSE_KEY.blue().bold(),
            QUIT_LABEL.into(),
            QUIT_KEY.blue().bold(),
        ])
    } else if selected_tab == SelectedTab::Streams {
        match streams_focus {
            StreamsFocus::Streams => Line::from(vec![
                NAV_KEYS_FULL.blue().bold(),
                TOGGLE_LOGS_LABEL.into(),
                TOGGLE_LOGS_KEY.blue().bold(),
                PAUSE_LABEL.into(),
                PAUSE_KEY.blue().bold(),
                QUIT_LABEL.into(),
                QUIT_KEY.blue().bold(),
            ]),
            StreamsFocus::Logs => Line::from(vec![
                NAV_KEYS_FULL.blue().bold(),
                TOGGLE_LOGS_LABEL.into(),
                TOGGLE_LOGS_KEY.blue().bold(),
                PAUSE_LABEL.into(),
                PAUSE_KEY.blue().bold(),
                INSPECT_LABEL.into(),
                INSPECT_KEY.blue().bold(),
                QUIT_LABEL.into(),
                QUIT_KEY.blue().bold(),
            ]),
            StreamsFocus::Inspect => Line::from(vec![
                NAV_KEYS_FULL.blue().bold(),
                TOGGLE_LOGS_LABEL.into(),
                TOGGLE_LOGS_KEY.blue().bold(),
                PAUSE_LABEL.into(),
                PAUSE_KEY.blue().bold(),
                CLOSE_LABEL.into(),
                CLOSE_KEYS.blue().bold(),
                QUIT_LABEL.into(),
                QUIT_KEY.blue().bold(),
            ]),
        }
    } else if selected_tab == SelectedTab::Channels {
        match channels_focus {
            ChannelsFocus::Channels => Line::from(vec![
                NAV_KEYS_FULL.blue().bold(),
                TOGGLE_LOGS_LABEL.into(),
                TOGGLE_LOGS_KEY.blue().bold(),
                PAUSE_LABEL.into(),
                PAUSE_KEY.blue().bold(),
                QUIT_LABEL.into(),
                QUIT_KEY.blue().bold(),
            ]),
            ChannelsFocus::Logs => Line::from(vec![
                NAV_KEYS_FULL.blue().bold(),
                TOGGLE_LOGS_LABEL.into(),
                TOGGLE_LOGS_KEY.blue().bold(),
                PAUSE_LABEL.into(),
                PAUSE_KEY.blue().bold(),
                INSPECT_LABEL.into(),
                INSPECT_KEY.blue().bold(),
                QUIT_LABEL.into(),
                QUIT_KEY.blue().bold(),
            ]),
            ChannelsFocus::Inspect => Line::from(vec![
                NAV_KEYS_FULL.blue().bold(),
                TOGGLE_LOGS_LABEL.into(),
                TOGGLE_LOGS_KEY.blue().bold(),
                PAUSE_LABEL.into(),
                PAUSE_KEY.blue().bold(),
                CLOSE_LABEL.into(),
                CLOSE_KEYS.blue().bold(),
                QUIT_LABEL.into(),
                QUIT_KEY.blue().bold(),
            ]),
        }
    } else {
        match functions_focus {
            FunctionsFocus::Functions => Line::from(vec![
                NAV_KEYS_FULL.blue().bold(),
                TOGGLE_LOGS_LABEL.into(),
                TOGGLE_LOGS_KEY.blue().bold(),
                PAUSE_LABEL.into(),
                PAUSE_KEY.blue().bold(),
                QUIT_LABEL.into(),
                QUIT_KEY.blue().bold(),
            ]),
            FunctionsFocus::Logs => Line::from(vec![
                NAV_KEYS_FULL.blue().bold(),
                TOGGLE_LOGS_LABEL.into(),
                TOGGLE_LOGS_KEY.blue().bold(),
                PAUSE_LABEL.into(),
                PAUSE_KEY.blue().bold(),
                QUIT_LABEL.into(),
                QUIT_KEY.blue().bold(),
            ]),
        }
    };

    let block = Block::bordered().border_set(border::PLAIN);

    let paragraph = Paragraph::new(controls_line).block(block).left_aligned();

    frame.render_widget(paragraph, area);
}
