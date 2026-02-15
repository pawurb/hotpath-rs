//! UI state management - navigation, selection, and focus handling

use crate::cmd::console::app::{
    App, DataFlowFocus, DebugFocus, FunctionsFocus, InspectedFunctionLog, SelectedTab,
};
use tracing::{debug, info};

#[hotpath::measure_all]
impl App {
    pub(crate) fn next_function(&mut self) {
        let function_count = self.active_functions().data.len();
        if function_count == 0 {
            return;
        }

        let table_state = self.active_table_state_mut();
        let i = match table_state.selected() {
            Some(i) => (i + 1).min(function_count - 1),
            None => 0,
        };
        table_state.select(Some(i));
    }

    pub(crate) fn previous_function(&mut self) {
        let function_count = self.active_functions().data.len();
        if function_count == 0 {
            return;
        }

        let table_state = self.active_table_state_mut();
        let i = match table_state.selected() {
            Some(i) => i.saturating_sub(1),
            None => 0,
        };
        table_state.select(Some(i));
    }

    pub(crate) fn toggle_pause(&mut self) {
        self.paused = !self.paused;
        info!("Paused: {}", self.paused);
    }

    pub(crate) fn switch_to_tab(&mut self, tab: SelectedTab) {
        debug!("Switching to tab: {}", tab.name());
        self.selected_tab = tab;
        self.request_refresh_for_current_tab();
    }

    pub(crate) fn toggle_function_logs(&mut self) {
        self.show_function_logs = !self.show_function_logs;
        if self.show_function_logs {
            self.pinned_function_id = self.selected_function_id();
            self.pinned_function = self.selected_function_name();
        } else {
            self.pinned_function = None;
            self.pinned_function_id = None;
            self.function_logs_table_state.select(None);
            self.functions_focus = FunctionsFocus::Functions;
        }
    }

    pub(crate) fn focus_functions(&mut self) {
        self.functions_focus = FunctionsFocus::Functions;
        self.function_logs_table_state.select(None);
    }

    fn function_logs_len(&self) -> usize {
        match self.selected_tab {
            SelectedTab::Timing => self
                .current_timing_logs
                .as_ref()
                .map(|l| l.logs.len())
                .unwrap_or(0),
            SelectedTab::Memory => self
                .current_alloc_logs
                .as_ref()
                .map(|l| l.logs.len())
                .unwrap_or(0),
            _ => 0,
        }
    }

    fn create_inspected_log_for_index(&self, i: usize) -> Option<InspectedFunctionLog> {
        match self.selected_tab {
            SelectedTab::Timing => self.current_timing_logs.as_ref().and_then(|logs| {
                logs.logs.get(i).map(|entry| InspectedFunctionLog {
                    invocation: entry.invocation,
                    value: entry.duration.clone(),
                    ago: entry.ago.clone(),
                    alloc_count: None,
                    tid: entry.thread_id,
                    result: entry.result.clone(),
                })
            }),
            SelectedTab::Memory => self.current_alloc_logs.as_ref().and_then(|logs| {
                logs.logs.get(i).map(|entry| InspectedFunctionLog {
                    invocation: entry.invocation,
                    value: entry.bytes.clone(),
                    ago: entry.ago.clone(),
                    alloc_count: entry.alloc_count,
                    tid: entry.thread_id,
                    result: entry.result.clone(),
                })
            }),
            _ => None,
        }
    }

    pub(crate) fn focus_function_logs(&mut self) {
        if !self.show_function_logs {
            self.toggle_function_logs();
        } else {
            let has_logs = self.function_logs_len() > 0;
            if has_logs {
                self.functions_focus = FunctionsFocus::Logs;
                if self.function_logs_table_state.selected().is_none() {
                    self.function_logs_table_state.select(Some(0));
                }
            }
        }
    }

    pub(crate) fn select_previous_function_log(&mut self) {
        let log_count = self.function_logs_len();
        if log_count > 0 {
            let i = match self.function_logs_table_state.selected() {
                Some(i) => i.saturating_sub(1),
                None => 0,
            };
            self.function_logs_table_state.select(Some(i));

            if self.functions_focus == FunctionsFocus::Inspect {
                self.inspected_function_log = self.create_inspected_log_for_index(i);
            }
        }
    }

    pub(crate) fn select_next_function_log(&mut self) {
        let log_count = self.function_logs_len();
        if log_count > 0 {
            let i = match self.function_logs_table_state.selected() {
                Some(i) => (i + 1).min(log_count - 1),
                None => 0,
            };
            self.function_logs_table_state.select(Some(i));

            if self.functions_focus == FunctionsFocus::Inspect {
                self.inspected_function_log = self.create_inspected_log_for_index(i);
            }
        }
    }

    pub(crate) fn toggle_function_inspect(&mut self) {
        if self.functions_focus == FunctionsFocus::Inspect {
            self.functions_focus = FunctionsFocus::Logs;
            self.inspected_function_log = None;
        } else if self.functions_focus == FunctionsFocus::Logs
            && self.function_logs_table_state.selected().is_some()
        {
            if let Some(selected) = self.function_logs_table_state.selected() {
                if let Some(inspected) = self.create_inspected_log_for_index(selected) {
                    self.inspected_function_log = Some(inspected);
                    self.functions_focus = FunctionsFocus::Inspect;
                }
            }
        }
    }

    pub(crate) fn close_function_inspect_and_refocus_functions(&mut self) {
        self.inspected_function_log = None;
        self.toggle_function_logs();
    }

    pub(crate) fn close_function_inspect_only(&mut self) {
        self.inspected_function_log = None;
        self.functions_focus = FunctionsFocus::Functions;
        self.function_logs_table_state.select(None);
    }

    // Data Flow tab state management

    pub(crate) fn select_previous_data_flow(&mut self) {
        let count = self.data_flow.entries.len();
        if count == 0 {
            return;
        }

        let i = match self.data_flow_table_state.selected() {
            Some(i) => i.saturating_sub(1),
            None => 0,
        };
        self.data_flow_table_state.select(Some(i));

        if self.paused && self.show_data_flow_logs {
            self.data_flow_logs = None;
        } else if self.show_data_flow_logs {
            self.request_data_flow_logs();
        }
    }

    pub(crate) fn select_next_data_flow(&mut self) {
        let count = self.data_flow.entries.len();
        if count == 0 {
            return;
        }

        let i = match self.data_flow_table_state.selected() {
            Some(i) => (i + 1).min(count - 1),
            None => 0,
        };
        self.data_flow_table_state.select(Some(i));

        if self.paused && self.show_data_flow_logs {
            self.data_flow_logs = None;
        } else if self.show_data_flow_logs {
            self.request_data_flow_logs();
        }
    }

    pub(crate) fn toggle_data_flow_logs(&mut self) {
        let has_valid_selection = self
            .data_flow_table_state
            .selected()
            .map(|i| i < self.data_flow.entries.len())
            .unwrap_or(false);

        if !self.data_flow.entries.is_empty() && has_valid_selection {
            if self.show_data_flow_logs {
                self.hide_data_flow_logs();
            } else {
                self.show_data_flow_logs = true;
                if self.paused {
                    self.data_flow_logs = None;
                } else {
                    self.request_data_flow_logs();
                }
            }
        }
    }

    pub(crate) fn hide_data_flow_logs(&mut self) {
        self.show_data_flow_logs = false;
        self.data_flow_logs = None;
        self.data_flow_logs_table_state.select(None);
        self.data_flow_focus = DataFlowFocus::List;
    }

    pub(crate) fn focus_data_flow_list(&mut self) {
        self.data_flow_focus = DataFlowFocus::List;
        self.data_flow_logs_table_state.select(None);
    }

    pub(crate) fn focus_data_flow_logs(&mut self) {
        if !self.show_data_flow_logs {
            self.toggle_data_flow_logs();
        } else if !self.data_flow.entries.is_empty() {
            if let Some(ref logs) = self.data_flow_logs {
                let has_logs = match logs {
                    crate::cmd::console::app::DataFlowLogs::Channel(l) => !l.sent_logs.is_empty(),
                    crate::cmd::console::app::DataFlowLogs::Stream(l) => !l.logs.is_empty(),
                    crate::cmd::console::app::DataFlowLogs::Future(l) => !l.calls.is_empty(),
                };
                if has_logs {
                    self.data_flow_focus = DataFlowFocus::Logs;
                    if self.data_flow_logs_table_state.selected().is_none() {
                        self.data_flow_logs_table_state.select(Some(0));
                    }
                }
            }
        }
    }

    fn data_flow_logs_len(&self) -> usize {
        match &self.data_flow_logs {
            Some(crate::cmd::console::app::DataFlowLogs::Channel(l)) => l.sent_logs.len(),
            Some(crate::cmd::console::app::DataFlowLogs::Stream(l)) => l.logs.len(),
            Some(crate::cmd::console::app::DataFlowLogs::Future(l)) => l.calls.len(),
            None => 0,
        }
    }

    pub(crate) fn select_previous_data_flow_log(&mut self) {
        let log_count = self.data_flow_logs_len();
        if log_count > 0 {
            let i = match self.data_flow_logs_table_state.selected() {
                Some(i) => i.saturating_sub(1),
                None => 0,
            };
            self.data_flow_logs_table_state.select(Some(i));

            if self.data_flow_focus == DataFlowFocus::Inspect {
                self.update_inspected_data_flow_log(i);
            }
        }
    }

    pub(crate) fn select_next_data_flow_log(&mut self) {
        let log_count = self.data_flow_logs_len();
        if log_count > 0 {
            let i = match self.data_flow_logs_table_state.selected() {
                Some(i) => (i + 1).min(log_count - 1),
                None => 0,
            };
            self.data_flow_logs_table_state.select(Some(i));

            if self.data_flow_focus == DataFlowFocus::Inspect {
                self.update_inspected_data_flow_log(i);
            }
        }
    }

    fn update_inspected_data_flow_log(&mut self, i: usize) {
        use crate::cmd::console::app::{DataFlowLogs, InspectedDataFlowLog};

        self.inspected_data_flow_log = match &self.data_flow_logs {
            Some(DataFlowLogs::Channel(logs)) => logs
                .sent_logs
                .get(i)
                .cloned()
                .map(InspectedDataFlowLog::ChannelSent),
            Some(DataFlowLogs::Stream(logs)) => {
                logs.logs.get(i).cloned().map(InspectedDataFlowLog::Stream)
            }
            Some(DataFlowLogs::Future(logs)) => logs
                .calls
                .get(i)
                .cloned()
                .map(InspectedDataFlowLog::FutureCall),
            None => None,
        };
    }

    pub(crate) fn toggle_data_flow_inspect(&mut self) {
        if self.data_flow_focus == DataFlowFocus::Inspect {
            self.data_flow_focus = DataFlowFocus::Logs;
            self.inspected_data_flow_log = None;
        } else if self.data_flow_focus == DataFlowFocus::Logs
            && self.data_flow_logs_table_state.selected().is_some()
        {
            if let Some(selected) = self.data_flow_logs_table_state.selected() {
                self.update_inspected_data_flow_log(selected);
                if self.inspected_data_flow_log.is_some() {
                    self.data_flow_focus = DataFlowFocus::Inspect;
                }
            }
        }
    }

    pub(crate) fn close_data_flow_inspect_and_refocus(&mut self) {
        self.inspected_data_flow_log = None;
        self.hide_data_flow_logs();
    }

    pub(crate) fn close_data_flow_inspect_only(&mut self) {
        self.inspected_data_flow_log = None;
        self.data_flow_focus = DataFlowFocus::List;
        self.data_flow_logs_table_state.select(None);
    }

    // Runtime tab state management

    pub(crate) fn select_previous_runtime_worker(&mut self) {
        let count = self
            .tokio_runtime
            .as_ref()
            .map(|s| s.workers.len())
            .unwrap_or(0);
        if count == 0 {
            return;
        }
        let i = match self.runtime_table_state.selected() {
            Some(i) => i.saturating_sub(1),
            None => 0,
        };
        self.runtime_table_state.select(Some(i));
    }

    pub(crate) fn select_next_runtime_worker(&mut self) {
        let count = self
            .tokio_runtime
            .as_ref()
            .map(|s| s.workers.len())
            .unwrap_or(0);
        if count == 0 {
            return;
        }
        let i = match self.runtime_table_state.selected() {
            Some(i) => (i + 1).min(count - 1),
            None => 0,
        };
        self.runtime_table_state.select(Some(i));
    }

    // Threads tab state management

    pub(crate) fn select_previous_thread(&mut self) {
        let count = self.threads.data.len();
        if count == 0 {
            return;
        }

        let i = match self.threads_table_state.selected() {
            Some(i) => i.saturating_sub(1),
            None => 0,
        };
        self.threads_table_state.select(Some(i));
    }

    pub(crate) fn select_next_thread(&mut self) {
        let count = self.threads.data.len();
        if count == 0 {
            return;
        }

        let i = match self.threads_table_state.selected() {
            Some(i) => (i + 1).min(count - 1),
            None => 0,
        };
        self.threads_table_state.select(Some(i));
    }

    // Debug tab state management

    pub(crate) fn select_previous_debug(&mut self) {
        let count = self.debug_stats.len();
        if count == 0 {
            return;
        }

        let i = match self.debug_table_state.selected() {
            Some(i) => i.saturating_sub(1),
            None => 0,
        };
        self.debug_table_state.select(Some(i));

        if self.paused && self.show_debug_logs {
            self.debug_logs = None;
        } else if self.show_debug_logs {
            self.request_debug_logs();
        }
    }

    pub(crate) fn select_next_debug(&mut self) {
        let count = self.debug_stats.len();
        if count == 0 {
            return;
        }

        let i = match self.debug_table_state.selected() {
            Some(i) => (i + 1).min(count - 1),
            None => 0,
        };
        self.debug_table_state.select(Some(i));

        if self.paused && self.show_debug_logs {
            self.debug_logs = None;
        } else if self.show_debug_logs {
            self.request_debug_logs();
        }
    }

    pub(crate) fn toggle_debug_logs(&mut self) {
        let has_valid_selection = self
            .debug_table_state
            .selected()
            .map(|i| i < self.debug_stats.len())
            .unwrap_or(false);

        if !self.debug_stats.is_empty() && has_valid_selection {
            if self.show_debug_logs {
                self.hide_debug_logs();
            } else {
                self.show_debug_logs = true;
                if self.paused {
                    self.debug_logs = None;
                } else {
                    self.request_debug_logs();
                }
            }
        }
    }

    pub(crate) fn hide_debug_logs(&mut self) {
        self.show_debug_logs = false;
        self.debug_logs = None;
        self.debug_logs_table_state.select(None);
        self.debug_focus = DebugFocus::Debug;
    }

    pub(crate) fn focus_debug(&mut self) {
        self.debug_focus = DebugFocus::Debug;
        self.debug_logs_table_state.select(None);
    }

    pub(crate) fn focus_debug_logs(&mut self) {
        if !self.show_debug_logs {
            self.toggle_debug_logs();
        } else if !self.debug_stats.is_empty() {
            if let Some(ref cached_logs) = self.debug_logs {
                if !cached_logs.is_empty() {
                    self.debug_focus = DebugFocus::Logs;
                    if self.debug_logs_table_state.selected().is_none() {
                        self.debug_logs_table_state.select(Some(0));
                    }
                }
            }
        }
    }

    pub(crate) fn select_previous_debug_log(&mut self) {
        if let Some(ref cached_logs) = self.debug_logs {
            let log_count = cached_logs.len();
            if log_count > 0 {
                let i = match self.debug_logs_table_state.selected() {
                    Some(i) => i.saturating_sub(1),
                    None => 0,
                };
                self.debug_logs_table_state.select(Some(i));

                if self.debug_focus == DebugFocus::Inspect {
                    let actual_idx = log_count - 1 - i;
                    if let Some(entry) = cached_logs.get(actual_idx) {
                        self.inspected_debug_log = Some(entry.clone());
                    }
                }
            }
        }
    }

    pub(crate) fn select_next_debug_log(&mut self) {
        if let Some(ref cached_logs) = self.debug_logs {
            let log_count = cached_logs.len();
            if log_count > 0 {
                let i = match self.debug_logs_table_state.selected() {
                    Some(i) => (i + 1).min(log_count - 1),
                    None => 0,
                };
                self.debug_logs_table_state.select(Some(i));

                if self.debug_focus == DebugFocus::Inspect {
                    let actual_idx = log_count - 1 - i;
                    if let Some(entry) = cached_logs.get(actual_idx) {
                        self.inspected_debug_log = Some(entry.clone());
                    }
                }
            }
        }
    }

    pub(crate) fn toggle_debug_inspect(&mut self) {
        if self.debug_focus == DebugFocus::Inspect {
            self.debug_focus = DebugFocus::Logs;
            self.inspected_debug_log = None;
        } else if self.debug_focus == DebugFocus::Logs
            && self.debug_logs_table_state.selected().is_some()
        {
            if let Some(selected) = self.debug_logs_table_state.selected() {
                if let Some(ref cached_logs) = self.debug_logs {
                    let log_count = cached_logs.len();
                    let actual_idx = log_count - 1 - selected;
                    if let Some(entry) = cached_logs.get(actual_idx) {
                        self.inspected_debug_log = Some(entry.clone());
                        self.debug_focus = DebugFocus::Inspect;
                    }
                }
            }
        }
    }

    pub(crate) fn close_debug_inspect_and_refocus_debug(&mut self) {
        self.inspected_debug_log = None;
        self.hide_debug_logs();
    }

    pub(crate) fn close_debug_inspect_only(&mut self) {
        self.inspected_debug_log = None;
        self.debug_focus = DebugFocus::Debug;
        self.debug_logs_table_state.select(None);
    }
}
