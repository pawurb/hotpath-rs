//! Keyboard input handling

use super::{App, Focus, SelectedTab};
use crossterm::event::KeyCode;

impl App {
    pub(crate) fn handle_key_event(&mut self, key_code: KeyCode) {
        match key_code {
            KeyCode::Char('q') | KeyCode::Char('Q') => self.exit(),
            KeyCode::Char('p') | KeyCode::Char('P') => self.toggle_pause(),
            KeyCode::Char('1') => {
                self.switch_to_tab(SelectedTab::Metrics);
                self.refresh_data();
            }
            KeyCode::Char('2') => {
                self.switch_to_tab(SelectedTab::Channels);
                self.refresh_data();
            }
            KeyCode::Char('o') | KeyCode::Char('O') => {
                if self.selected_tab == SelectedTab::Channels {
                    match self.focus {
                        Focus::Inspect => self.close_inspect_and_refocus_channels(),
                        Focus::Logs => self.hide_logs(),
                        Focus::Channels => self.toggle_logs(),
                    }
                } else {
                    self.toggle_samples();
                    self.fetch_samples_if_open(self.metrics_port);
                }
            }
            KeyCode::Left | KeyCode::Char('h') | KeyCode::Char('H') => {
                if self.selected_tab == SelectedTab::Channels {
                    if self.focus == Focus::Inspect {
                        self.close_inspect_only();
                    } else {
                        self.focus_channels();
                    }
                }
            }
            KeyCode::Right | KeyCode::Char('l') => {
                if self.selected_tab == SelectedTab::Channels {
                    self.focus_logs();
                }
            }
            KeyCode::Char('i') | KeyCode::Char('I') => {
                if self.selected_tab == SelectedTab::Channels {
                    self.toggle_inspect();
                }
            }
            KeyCode::Char('j') | KeyCode::Down => {
                if self.selected_tab == SelectedTab::Channels {
                    match self.focus {
                        Focus::Channels => self.select_next_channel(),
                        Focus::Logs | Focus::Inspect => self.select_next_log(),
                    }
                } else {
                    self.next_function();
                    self.update_and_fetch_samples(self.metrics_port);
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if self.selected_tab == SelectedTab::Channels {
                    match self.focus {
                        Focus::Channels => self.select_previous_channel(),
                        Focus::Logs | Focus::Inspect => self.select_previous_log(),
                    }
                } else {
                    self.previous_function();
                    self.update_and_fetch_samples(self.metrics_port);
                }
            }
            _ => {}
        }
    }
}
