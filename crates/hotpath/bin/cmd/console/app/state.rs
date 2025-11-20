//! UI state management - navigation, selection, and focus handling

use super::{App, Focus, SelectedTab};

impl App {
    pub(crate) fn next_function(&mut self) {
        let function_count = self.metrics.data.0.len();
        if function_count == 0 {
            return;
        }

        let i = match self.table_state.selected() {
            Some(i) => (i + 1).min(function_count - 1), // Bounded, stop at last
            None => 0,
        };
        self.table_state.select(Some(i));
    }

    pub(crate) fn previous_function(&mut self) {
        let function_count = self.metrics.data.0.len();
        if function_count == 0 {
            return;
        }

        let i = match self.table_state.selected() {
            Some(i) => i.saturating_sub(1), // Bounded, stop at 0
            None => 0,
        };
        self.table_state.select(Some(i));
    }

    pub(crate) fn toggle_pause(&mut self) {
        self.paused = !self.paused;
    }

    pub(crate) fn switch_to_tab(&mut self, tab: SelectedTab) {
        self.selected_tab = tab;
    }

    pub(crate) fn select_previous_channel(&mut self) {
        let count = self.channels.channels.len();
        if count == 0 {
            return;
        }

        let i = match self.table_state.selected() {
            Some(i) => i.saturating_sub(1),
            None => 0,
        };
        self.table_state.select(Some(i));

        if self.paused && self.show_logs {
            self.logs = None;
        } else if self.show_logs {
            self.refresh_logs();
        }
    }

    pub(crate) fn select_next_channel(&mut self) {
        let count = self.channels.channels.len();
        if count == 0 {
            return;
        }

        let i = match self.table_state.selected() {
            Some(i) => (i + 1).min(count - 1),
            None => 0,
        };
        self.table_state.select(Some(i));

        if self.paused && self.show_logs {
            self.logs = None;
        } else if self.show_logs {
            self.refresh_logs();
        }
    }

    pub(crate) fn toggle_logs(&mut self) {
        let has_valid_selection = self
            .table_state
            .selected()
            .map(|i| i < self.channels.channels.len())
            .unwrap_or(false);

        if !self.channels.channels.is_empty() && has_valid_selection {
            if self.show_logs {
                self.hide_logs();
            } else {
                self.show_logs = true;
                if self.paused {
                    self.logs = None;
                } else {
                    self.refresh_logs();
                }
            }
        }
    }

    pub(crate) fn hide_logs(&mut self) {
        self.show_logs = false;
        self.logs = None;
        self.logs_table_state.select(None);
        self.focus = Focus::Channels;
    }

    pub(crate) fn focus_channels(&mut self) {
        self.focus = Focus::Channels;
        self.logs_table_state.select(None);
    }

    pub(crate) fn focus_logs(&mut self) {
        if !self.show_logs {
            self.toggle_logs();
        } else if !self.channels.channels.is_empty() {
            if let Some(ref cached_logs) = self.logs {
                if !cached_logs.logs.sent_logs.is_empty() {
                    self.focus = Focus::Logs;
                    if self.logs_table_state.selected().is_none() {
                        self.logs_table_state.select(Some(0));
                    }
                }
            }
        }
    }

    pub(crate) fn select_previous_log(&mut self) {
        if let Some(ref cached_logs) = self.logs {
            let log_count = cached_logs.logs.sent_logs.len();
            if log_count > 0 {
                let i = match self.logs_table_state.selected() {
                    Some(i) => i.saturating_sub(1),
                    None => 0,
                };
                self.logs_table_state.select(Some(i));

                // Update inspected log if inspect popup is open
                if self.focus == Focus::Inspect {
                    if let Some(entry) = cached_logs.logs.sent_logs.get(i) {
                        self.inspected_log = Some(entry.clone());
                    }
                }
            }
        }
    }

    pub(crate) fn select_next_log(&mut self) {
        if let Some(ref cached_logs) = self.logs {
            let log_count = cached_logs.logs.sent_logs.len();
            if log_count > 0 {
                let i = match self.logs_table_state.selected() {
                    Some(i) => (i + 1).min(log_count - 1),
                    None => 0,
                };
                self.logs_table_state.select(Some(i));

                // Update inspected log if inspect popup is open
                if self.focus == Focus::Inspect {
                    if let Some(entry) = cached_logs.logs.sent_logs.get(i) {
                        self.inspected_log = Some(entry.clone());
                    }
                }
            }
        }
    }

    pub(crate) fn toggle_inspect(&mut self) {
        if self.focus == Focus::Inspect {
            // Closing inspect popup
            self.focus = Focus::Logs;
            self.inspected_log = None;
        } else if self.focus == Focus::Logs && self.logs_table_state.selected().is_some() {
            // Opening inspect popup - capture the current log entry
            if let Some(selected) = self.logs_table_state.selected() {
                if let Some(ref cached_logs) = self.logs {
                    if let Some(entry) = cached_logs.logs.sent_logs.get(selected) {
                        self.inspected_log = Some(entry.clone());
                        self.focus = Focus::Inspect;
                    }
                }
            }
        }
    }

    pub(crate) fn close_inspect_and_refocus_channels(&mut self) {
        self.inspected_log = None;
        self.hide_logs();
    }

    pub(crate) fn close_inspect_only(&mut self) {
        self.inspected_log = None;
        self.focus = Focus::Channels;
        self.logs_table_state.select(None);
    }

    pub(crate) fn toggle_samples(&mut self) {
        self.show_samples = !self.show_samples;
        if self.show_samples {
            // Pin the currently selected function when opening samples panel
            self.pinned_function = self.selected_function_name();
        } else {
            // Clear pinned function when closing samples panel
            self.pinned_function = None;
        }
    }
}
