use crate::cmd::console::app::{DataFlowFocus, DebugFocus, FunctionsFocus, SelectedTab};
use ratatui::{
    layout::{Alignment, Rect},
    style::Stylize,
    symbols::border,
    text::Line,
    widgets::{Block, Paragraph},
    Frame,
};

const NAV_KEYS_FULL: &str = " <←↑↓→/hjkl> <gg,G> ";
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
const SUBTAB_LABEL: &str = " | Toggle Mode ";
const SUBTAB_KEY: &str = "<1> ";
const VERSION: &str = concat!("v", env!("CARGO_PKG_VERSION"));

#[hotpath::measure]
pub(crate) fn render_help_bar(
    frame: &mut Frame,
    area: Rect,
    selected_tab: SelectedTab,
    data_flow_focus: DataFlowFocus,
    functions_focus: FunctionsFocus,
    debug_focus: DebugFocus,
) {
    let controls_line =
        if selected_tab == SelectedTab::Threads || selected_tab == SelectedTab::Runtime {
            Line::from(vec![
                NAV_KEYS_FULL.blue().bold(),
                PAUSE_LABEL.into(),
                PAUSE_KEY.blue().bold(),
                QUIT_LABEL.into(),
                QUIT_KEY.blue().bold(),
            ])
        } else if selected_tab == SelectedTab::DataFlow {
            match data_flow_focus {
                DataFlowFocus::List => Line::from(vec![
                    NAV_KEYS_FULL.blue().bold(),
                    TOGGLE_LOGS_LABEL.into(),
                    TOGGLE_LOGS_KEY.blue().bold(),
                    PAUSE_LABEL.into(),
                    PAUSE_KEY.blue().bold(),
                    QUIT_LABEL.into(),
                    QUIT_KEY.blue().bold(),
                ]),
                DataFlowFocus::Logs => Line::from(vec![
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
                DataFlowFocus::Inspect => Line::from(vec![
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
        } else if selected_tab == SelectedTab::Debug {
            match debug_focus {
                DebugFocus::Debug => Line::from(vec![
                    NAV_KEYS_FULL.blue().bold(),
                    TOGGLE_LOGS_LABEL.into(),
                    TOGGLE_LOGS_KEY.blue().bold(),
                    PAUSE_LABEL.into(),
                    PAUSE_KEY.blue().bold(),
                    QUIT_LABEL.into(),
                    QUIT_KEY.blue().bold(),
                ]),
                DebugFocus::Logs => Line::from(vec![
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
                DebugFocus::Inspect => Line::from(vec![
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
                    SUBTAB_LABEL.into(),
                    SUBTAB_KEY.blue().bold(),
                    TOGGLE_LOGS_LABEL.into(),
                    TOGGLE_LOGS_KEY.blue().bold(),
                    PAUSE_LABEL.into(),
                    PAUSE_KEY.blue().bold(),
                    QUIT_LABEL.into(),
                    QUIT_KEY.blue().bold(),
                ]),
                FunctionsFocus::Logs => Line::from(vec![
                    NAV_KEYS_FULL.blue().bold(),
                    SUBTAB_LABEL.into(),
                    SUBTAB_KEY.blue().bold(),
                    TOGGLE_LOGS_LABEL.into(),
                    TOGGLE_LOGS_KEY.blue().bold(),
                    INSPECT_LABEL.into(),
                    INSPECT_KEY.blue().bold(),
                    PAUSE_LABEL.into(),
                    PAUSE_KEY.blue().bold(),
                    QUIT_LABEL.into(),
                    QUIT_KEY.blue().bold(),
                ]),
                FunctionsFocus::Inspect => Line::from(vec![
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
        };

    let block = Block::bordered()
        .border_set(border::PLAIN)
        .title_bottom(Line::from(format!(" {VERSION} ")).alignment(Alignment::Right));

    let paragraph = Paragraph::new(controls_line).block(block).left_aligned();

    frame.render_widget(paragraph, area);
}
