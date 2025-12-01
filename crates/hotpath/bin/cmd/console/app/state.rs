//! UI state management - navigation, selection, and focus handling

use super::{
    App, ChannelsFocus, FunctionsFocus, FuturesFocus, InspectedFunctionLog, SelectedTab,
    StreamsFocus,
};

#[cfg_attr(feature = "hotpath", hotpath::measure_all)]
impl App {
    pub(crate) fn next_function(&mut self) {
        let function_count = self.active_functions().data.0.len();
        if function_count == 0 {
            return;
        }

        let table_state = self.active_table_state_mut();
        let i = match table_state.selected() {
            Some(i) => (i + 1).min(function_count - 1), // Bounded, stop at last
            None => 0,
        };
        table_state.select(Some(i));
    }

    pub(crate) fn previous_function(&mut self) {
        let function_count = self.active_functions().data.0.len();
        if function_count == 0 {
            return;
        }

        let table_state = self.active_table_state_mut();
        let i = match table_state.selected() {
            Some(i) => i.saturating_sub(1), // Bounded, stop at 0
            None => 0,
        };
        table_state.select(Some(i));
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

        let i = match self.channels_table_state.selected() {
            Some(i) => i.saturating_sub(1),
            None => 0,
        };
        self.channels_table_state.select(Some(i));

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

        let i = match self.channels_table_state.selected() {
            Some(i) => (i + 1).min(count - 1),
            None => 0,
        };
        self.channels_table_state.select(Some(i));

        if self.paused && self.show_logs {
            self.logs = None;
        } else if self.show_logs {
            self.refresh_logs();
        }
    }

    pub(crate) fn toggle_logs(&mut self) {
        let has_valid_selection = self
            .channels_table_state
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
        self.channel_logs_table_state.select(None);
        self.channels_focus = ChannelsFocus::Channels;
    }

    pub(crate) fn focus_channels(&mut self) {
        self.channels_focus = ChannelsFocus::Channels;
        self.channel_logs_table_state.select(None);
    }

    pub(crate) fn focus_logs(&mut self) {
        if !self.show_logs {
            self.toggle_logs();
        } else if !self.channels.channels.is_empty() {
            if let Some(ref cached_logs) = self.logs {
                if !cached_logs.logs.sent_logs.is_empty() {
                    self.channels_focus = ChannelsFocus::Logs;
                    if self.channel_logs_table_state.selected().is_none() {
                        self.channel_logs_table_state.select(Some(0));
                    }
                }
            }
        }
    }

    pub(crate) fn select_previous_log(&mut self) {
        if let Some(ref cached_logs) = self.logs {
            let log_count = cached_logs.logs.sent_logs.len();
            if log_count > 0 {
                let i = match self.channel_logs_table_state.selected() {
                    Some(i) => i.saturating_sub(1),
                    None => 0,
                };
                self.channel_logs_table_state.select(Some(i));

                // Update inspected log if inspect popup is open
                if self.channels_focus == ChannelsFocus::Inspect {
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
                let i = match self.channel_logs_table_state.selected() {
                    Some(i) => (i + 1).min(log_count - 1),
                    None => 0,
                };
                self.channel_logs_table_state.select(Some(i));

                // Update inspected log if inspect popup is open
                if self.channels_focus == ChannelsFocus::Inspect {
                    if let Some(entry) = cached_logs.logs.sent_logs.get(i) {
                        self.inspected_log = Some(entry.clone());
                    }
                }
            }
        }
    }

    pub(crate) fn toggle_inspect(&mut self) {
        if self.channels_focus == ChannelsFocus::Inspect {
            // Closing inspect popup
            self.channels_focus = ChannelsFocus::Logs;
            self.inspected_log = None;
        } else if self.channels_focus == ChannelsFocus::Logs
            && self.channel_logs_table_state.selected().is_some()
        {
            // Opening inspect popup - capture the current log entry
            if let Some(selected) = self.channel_logs_table_state.selected() {
                if let Some(ref cached_logs) = self.logs {
                    if let Some(entry) = cached_logs.logs.sent_logs.get(selected) {
                        self.inspected_log = Some(entry.clone());
                        self.channels_focus = ChannelsFocus::Inspect;
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
        self.channels_focus = ChannelsFocus::Channels;
        self.channel_logs_table_state.select(None);
    }

    pub(crate) fn toggle_function_logs(&mut self) {
        self.show_function_logs = !self.show_function_logs;
        if self.show_function_logs {
            // Pin the currently selected function when opening function logs panel
            self.pinned_function = self.selected_function_name();
        } else {
            // Clear pinned function when closing function logs panel
            self.pinned_function = None;
            self.function_logs_table_state.select(None);
            self.functions_focus = FunctionsFocus::Functions;
        }
    }

    pub(crate) fn focus_functions(&mut self) {
        self.functions_focus = FunctionsFocus::Functions;
        self.function_logs_table_state.select(None);
    }

    pub(crate) fn focus_function_logs(&mut self) {
        if !self.show_function_logs {
            self.toggle_function_logs();
        } else if let Some(ref function_logs) = self.current_function_logs {
            if !function_logs.logs.is_empty() {
                self.functions_focus = FunctionsFocus::Logs;
                if self.function_logs_table_state.selected().is_none() {
                    self.function_logs_table_state.select(Some(0));
                }
            }
        }
    }

    pub(crate) fn select_previous_function_log(&mut self) {
        if let Some(ref function_logs) = self.current_function_logs {
            let log_count = function_logs.logs.len();
            if log_count > 0 {
                let i = match self.function_logs_table_state.selected() {
                    Some(i) => i.saturating_sub(1),
                    None => 0,
                };
                self.function_logs_table_state.select(Some(i));

                // Update inspected log if inspect popup is open
                if self.functions_focus == FunctionsFocus::Inspect {
                    let total_invocations = function_logs.count;
                    let invocation_number = total_invocations - i;
                    if let Some(entry) = function_logs.logs.get(i) {
                        self.inspected_function_log = Some(InspectedFunctionLog {
                            invocation_index: invocation_number,
                            value: entry.value,
                            elapsed_nanos: entry.elapsed_nanos,
                            alloc_count: entry.alloc_count,
                            tid: entry.tid,
                            result: entry.result.clone(),
                        });
                    }
                }
            }
        }
    }

    pub(crate) fn select_next_function_log(&mut self) {
        if let Some(ref function_logs) = self.current_function_logs {
            let log_count = function_logs.logs.len();
            if log_count > 0 {
                let i = match self.function_logs_table_state.selected() {
                    Some(i) => (i + 1).min(log_count - 1),
                    None => 0,
                };
                self.function_logs_table_state.select(Some(i));

                // Update inspected log if inspect popup is open
                if self.functions_focus == FunctionsFocus::Inspect {
                    let total_invocations = function_logs.count;
                    let invocation_number = total_invocations - i;
                    if let Some(entry) = function_logs.logs.get(i) {
                        self.inspected_function_log = Some(InspectedFunctionLog {
                            invocation_index: invocation_number,
                            value: entry.value,
                            elapsed_nanos: entry.elapsed_nanos,
                            alloc_count: entry.alloc_count,
                            tid: entry.tid,
                            result: entry.result.clone(),
                        });
                    }
                }
            }
        }
    }

    pub(crate) fn toggle_function_inspect(&mut self) {
        if self.functions_focus == FunctionsFocus::Inspect {
            // Closing inspect popup
            self.functions_focus = FunctionsFocus::Logs;
            self.inspected_function_log = None;
        } else if self.functions_focus == FunctionsFocus::Logs
            && self.function_logs_table_state.selected().is_some()
        {
            // Opening inspect popup - capture the current log entry
            if let Some(selected) = self.function_logs_table_state.selected() {
                if let Some(ref function_logs) = self.current_function_logs {
                    if let Some(entry) = function_logs.logs.get(selected) {
                        let total_invocations = function_logs.count;
                        let invocation_number = total_invocations - selected;
                        self.inspected_function_log = Some(InspectedFunctionLog {
                            invocation_index: invocation_number,
                            value: entry.value,
                            elapsed_nanos: entry.elapsed_nanos,
                            alloc_count: entry.alloc_count,
                            tid: entry.tid,
                            result: entry.result.clone(),
                        });
                        self.functions_focus = FunctionsFocus::Inspect;
                    }
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

    // Streams state management methods
    pub(crate) fn select_previous_stream(&mut self) {
        let count = self.streams.streams.len();
        if count == 0 {
            return;
        }

        let i = match self.streams_table_state.selected() {
            Some(i) => i.saturating_sub(1),
            None => 0,
        };
        self.streams_table_state.select(Some(i));

        if self.paused && self.show_stream_logs {
            self.stream_logs = None;
        } else if self.show_stream_logs {
            self.refresh_stream_logs();
        }
    }

    pub(crate) fn select_next_stream(&mut self) {
        let count = self.streams.streams.len();
        if count == 0 {
            return;
        }

        let i = match self.streams_table_state.selected() {
            Some(i) => (i + 1).min(count - 1),
            None => 0,
        };
        self.streams_table_state.select(Some(i));

        if self.paused && self.show_stream_logs {
            self.stream_logs = None;
        } else if self.show_stream_logs {
            self.refresh_stream_logs();
        }
    }

    pub(crate) fn toggle_stream_logs(&mut self) {
        let has_valid_selection = self
            .streams_table_state
            .selected()
            .map(|i| i < self.streams.streams.len())
            .unwrap_or(false);

        if !self.streams.streams.is_empty() && has_valid_selection {
            if self.show_stream_logs {
                self.hide_stream_logs();
            } else {
                self.show_stream_logs = true;
                if self.paused {
                    self.stream_logs = None;
                } else {
                    self.refresh_stream_logs();
                }
            }
        }
    }

    pub(crate) fn hide_stream_logs(&mut self) {
        self.show_stream_logs = false;
        self.stream_logs = None;
        self.stream_logs_table_state.select(None);
        self.streams_focus = StreamsFocus::Streams;
    }

    pub(crate) fn focus_streams(&mut self) {
        self.streams_focus = StreamsFocus::Streams;
        self.stream_logs_table_state.select(None);
    }

    pub(crate) fn focus_stream_logs(&mut self) {
        if !self.show_stream_logs {
            self.toggle_stream_logs();
        } else if !self.streams.streams.is_empty() {
            if let Some(ref cached_logs) = self.stream_logs {
                if !cached_logs.logs.logs.is_empty() {
                    self.streams_focus = StreamsFocus::Logs;
                    if self.stream_logs_table_state.selected().is_none() {
                        self.stream_logs_table_state.select(Some(0));
                    }
                }
            }
        }
    }

    pub(crate) fn select_previous_stream_log(&mut self) {
        if let Some(ref cached_logs) = self.stream_logs {
            let log_count = cached_logs.logs.logs.len();
            if log_count > 0 {
                let i = match self.stream_logs_table_state.selected() {
                    Some(i) => i.saturating_sub(1),
                    None => 0,
                };
                self.stream_logs_table_state.select(Some(i));

                // Update inspected log if inspect popup is open
                if self.streams_focus == StreamsFocus::Inspect {
                    if let Some(entry) = cached_logs.logs.logs.get(i) {
                        self.inspected_stream_log = Some(entry.clone());
                    }
                }
            }
        }
    }

    pub(crate) fn select_next_stream_log(&mut self) {
        if let Some(ref cached_logs) = self.stream_logs {
            let log_count = cached_logs.logs.logs.len();
            if log_count > 0 {
                let i = match self.stream_logs_table_state.selected() {
                    Some(i) => (i + 1).min(log_count - 1),
                    None => 0,
                };
                self.stream_logs_table_state.select(Some(i));

                // Update inspected log if inspect popup is open
                if self.streams_focus == StreamsFocus::Inspect {
                    if let Some(entry) = cached_logs.logs.logs.get(i) {
                        self.inspected_stream_log = Some(entry.clone());
                    }
                }
            }
        }
    }

    pub(crate) fn toggle_stream_inspect(&mut self) {
        if self.streams_focus == StreamsFocus::Inspect {
            // Closing inspect popup
            self.streams_focus = StreamsFocus::Logs;
            self.inspected_stream_log = None;
        } else if self.streams_focus == StreamsFocus::Logs
            && self.stream_logs_table_state.selected().is_some()
        {
            // Opening inspect popup - capture the current log entry
            if let Some(selected) = self.stream_logs_table_state.selected() {
                if let Some(ref cached_logs) = self.stream_logs {
                    if let Some(entry) = cached_logs.logs.logs.get(selected) {
                        self.inspected_stream_log = Some(entry.clone());
                        self.streams_focus = StreamsFocus::Inspect;
                    }
                }
            }
        }
    }

    pub(crate) fn close_stream_inspect_and_refocus_streams(&mut self) {
        self.inspected_stream_log = None;
        self.hide_stream_logs();
    }

    pub(crate) fn close_stream_inspect_only(&mut self) {
        self.inspected_stream_log = None;
        self.streams_focus = StreamsFocus::Streams;
        self.stream_logs_table_state.select(None);
    }

    // Threads state management methods
    pub(crate) fn select_previous_thread(&mut self) {
        let count = self.threads.threads.len();
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
        let count = self.threads.threads.len();
        if count == 0 {
            return;
        }

        let i = match self.threads_table_state.selected() {
            Some(i) => (i + 1).min(count - 1),
            None => 0,
        };
        self.threads_table_state.select(Some(i));
    }

    // Futures state management methods
    pub(crate) fn select_previous_future(&mut self) {
        let count = self.futures.futures.len();
        if count == 0 {
            return;
        }

        let i = match self.futures_table_state.selected() {
            Some(i) => i.saturating_sub(1),
            None => 0,
        };
        self.futures_table_state.select(Some(i));

        if self.paused && self.show_future_calls {
            self.future_calls = None;
        } else if self.show_future_calls {
            self.refresh_future_calls();
        }
    }

    pub(crate) fn select_next_future(&mut self) {
        let count = self.futures.futures.len();
        if count == 0 {
            return;
        }

        let i = match self.futures_table_state.selected() {
            Some(i) => (i + 1).min(count - 1),
            None => 0,
        };
        self.futures_table_state.select(Some(i));

        if self.paused && self.show_future_calls {
            self.future_calls = None;
        } else if self.show_future_calls {
            self.refresh_future_calls();
        }
    }

    pub(crate) fn toggle_future_calls(&mut self) {
        let has_valid_selection = self
            .futures_table_state
            .selected()
            .map(|i| i < self.futures.futures.len())
            .unwrap_or(false);

        if !self.futures.futures.is_empty() && has_valid_selection {
            if self.show_future_calls {
                self.hide_future_calls();
            } else {
                self.show_future_calls = true;
                if self.paused {
                    self.future_calls = None;
                } else {
                    self.refresh_future_calls();
                }
            }
        }
    }

    pub(crate) fn hide_future_calls(&mut self) {
        self.show_future_calls = false;
        self.future_calls = None;
        self.future_calls_table_state.select(None);
        self.futures_focus = FuturesFocus::Futures;
    }

    pub(crate) fn focus_futures(&mut self) {
        self.futures_focus = FuturesFocus::Futures;
        self.future_calls_table_state.select(None);
    }

    pub(crate) fn focus_future_calls(&mut self) {
        if !self.show_future_calls {
            self.toggle_future_calls();
        } else if !self.futures.futures.is_empty() {
            if let Some(ref future_calls) = self.future_calls {
                if !future_calls.calls.is_empty() {
                    self.futures_focus = FuturesFocus::Calls;
                    if self.future_calls_table_state.selected().is_none() {
                        self.future_calls_table_state.select(Some(0));
                    }
                }
            }
        }
    }

    pub(crate) fn select_next_future_call(&mut self) {
        if let Some(ref future_calls) = self.future_calls {
            let count = future_calls.calls.len();
            if count > 0 {
                let i = match self.future_calls_table_state.selected() {
                    Some(i) => (i + 1).min(count - 1),
                    None => 0,
                };
                self.future_calls_table_state.select(Some(i));

                // Update inspected call if inspect popup is open
                if self.futures_focus == FuturesFocus::Inspect {
                    if let Some(call) = future_calls.calls.get(i) {
                        self.inspected_future_call = Some(call.clone());
                    }
                }
            }
        }
    }

    pub(crate) fn select_previous_future_call(&mut self) {
        if let Some(ref future_calls) = self.future_calls {
            let count = future_calls.calls.len();
            if count > 0 {
                let i = match self.future_calls_table_state.selected() {
                    Some(i) => i.saturating_sub(1),
                    None => 0,
                };
                self.future_calls_table_state.select(Some(i));

                // Update inspected call if inspect popup is open
                if self.futures_focus == FuturesFocus::Inspect {
                    if let Some(call) = future_calls.calls.get(i) {
                        self.inspected_future_call = Some(call.clone());
                    }
                }
            }
        }
    }

    pub(crate) fn toggle_future_inspect(&mut self) {
        if self.futures_focus == FuturesFocus::Inspect {
            // Closing inspect popup
            self.futures_focus = FuturesFocus::Calls;
            self.inspected_future_call = None;
        } else if self.futures_focus == FuturesFocus::Calls
            && self.future_calls_table_state.selected().is_some()
        {
            // Opening inspect popup - capture the current call
            if let Some(selected) = self.future_calls_table_state.selected() {
                if let Some(ref future_calls) = self.future_calls {
                    if let Some(call) = future_calls.calls.get(selected) {
                        self.inspected_future_call = Some(call.clone());
                        self.futures_focus = FuturesFocus::Inspect;
                    }
                }
            }
        }
    }

    pub(crate) fn close_future_inspect_and_refocus_futures(&mut self) {
        self.inspected_future_call = None;
        self.hide_future_calls();
    }

    pub(crate) fn close_future_inspect_only(&mut self) {
        self.inspected_future_call = None;
        self.futures_focus = FuturesFocus::Futures;
        self.future_calls_table_state.select(None);
    }
}
