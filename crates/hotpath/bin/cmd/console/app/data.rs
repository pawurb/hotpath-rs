//! Data management - fetching, updating, and transforming functions/channels

use super::{App, CachedLogs, CachedStreamLogs, SelectedTab};
use hotpath::streams::StreamsJson;
use hotpath::threads::ThreadsJson;
use hotpath::{FunctionLogsJson, FunctionsJson};
use std::collections::HashMap;
use std::time::Instant;

#[cfg_attr(feature = "hotpath", hotpath::measure_all)]
impl App {
    pub(crate) fn update_timing_metrics(&mut self, metrics: FunctionsJson) {
        // Capture the currently selected function name (not index!)
        let selected_function_name = self.selected_function_name();

        self.timing_functions = metrics;
        self.last_successful_fetch = Some(Instant::now());
        self.error_message = None;

        let sorted_entries = Self::get_sorted_measurements_for(&self.timing_functions);

        if let Some(function_name) = selected_function_name {
            // Find the new index of the previously selected function in sorted order
            if let Some(new_idx) = sorted_entries
                .iter()
                .position(|(name, _)| name == &function_name)
            {
                self.timing_table_state.select(Some(new_idx));
            } else {
                // Function no longer exists, select the last one
                if !sorted_entries.is_empty() {
                    self.timing_table_state
                        .select(Some(sorted_entries.len() - 1));
                }
            }
        } else if let Some(selected) = self.timing_table_state.selected() {
            // Bound check: if current selection is now out of bounds
            if selected >= sorted_entries.len() && !sorted_entries.is_empty() {
                self.timing_table_state
                    .select(Some(sorted_entries.len() - 1));
            }
        } else if !sorted_entries.is_empty() {
            // No selection yet, select first item
            self.timing_table_state.select(Some(0));
        }
    }

    pub(crate) fn update_memory_metrics(&mut self, metrics: FunctionsJson) {
        // Capture the currently selected function name (not index!)
        let selected_function_name = self.selected_function_name();

        self.memory_functions = metrics;
        self.last_successful_fetch = Some(Instant::now());
        self.error_message = None;

        let sorted_entries = Self::get_sorted_measurements_for(&self.memory_functions);

        if let Some(function_name) = selected_function_name {
            // Find the new index of the previously selected function in sorted order
            if let Some(new_idx) = sorted_entries
                .iter()
                .position(|(name, _)| name == &function_name)
            {
                self.memory_table_state.select(Some(new_idx));
            } else {
                // Function no longer exists, select the last one
                if !sorted_entries.is_empty() {
                    self.memory_table_state
                        .select(Some(sorted_entries.len() - 1));
                }
            }
        } else if let Some(selected) = self.memory_table_state.selected() {
            // Bound check: if current selection is now out of bounds
            if selected >= sorted_entries.len() && !sorted_entries.is_empty() {
                self.memory_table_state
                    .select(Some(sorted_entries.len() - 1));
            }
        } else if !sorted_entries.is_empty() {
            // No selection yet, select first item
            self.memory_table_state.select(Some(0));
        }
    }

    pub(crate) fn set_error(&mut self, error: String) {
        self.error_message = Some(error);
    }

    pub(crate) fn update_channels(&mut self, channels: hotpath::channels::ChannelsJson) {
        // Capture the currently selected channel ID (not index!)
        let selected_channel_id = self
            .channels_table_state
            .selected()
            .and_then(|idx| self.channels.channels.get(idx))
            .map(|stat| stat.id);

        self.channels = channels;
        self.last_successful_fetch = Some(Instant::now());
        self.error_message = None;

        // Try to restore selection to the same channel ID
        if let Some(channel_id) = selected_channel_id {
            // Find the new index of the previously selected channel
            if let Some(new_idx) = self
                .channels
                .channels
                .iter()
                .position(|stat| stat.id == channel_id)
            {
                self.channels_table_state.select(Some(new_idx));
            } else {
                // Channel no longer exists, select the last one if available
                if !self.channels.channels.is_empty() {
                    self.channels_table_state
                        .select(Some(self.channels.channels.len() - 1));
                }
            }
        } else if let Some(selected) = self.channels_table_state.selected() {
            if selected >= self.channels.channels.len() && !self.channels.channels.is_empty() {
                self.channels_table_state
                    .select(Some(self.channels.channels.len() - 1));
            }
        }

        if self.show_logs {
            self.refresh_logs();
        }
    }

    pub(crate) fn refresh_logs(&mut self) {
        if self.paused {
            return;
        }

        self.logs = None;

        if let Some(selected) = self.channels_table_state.selected() {
            if !self.channels.channels.is_empty() && selected < self.channels.channels.len() {
                let channel_id = self.channels.channels[selected].id;
                if let Ok(logs) = super::super::http::fetch_channel_logs(
                    &self.agent,
                    self.metrics_port,
                    channel_id,
                ) {
                    let received_map: HashMap<u64, hotpath::channels::LogEntry> = logs
                        .received_logs
                        .iter()
                        .map(|entry| (entry.index, entry.clone()))
                        .collect();

                    self.logs = Some(CachedLogs { logs, received_map });

                    // Ensure logs table selection is valid
                    if let Some(ref cached_logs) = self.logs {
                        let log_count = cached_logs.logs.sent_logs.len();
                        if let Some(selected) = self.channel_logs_table_state.selected() {
                            if selected >= log_count && log_count > 0 {
                                self.channel_logs_table_state.select(Some(log_count - 1));
                            }
                        }
                    }
                }
            }
        }
    }

    /// Get sorted entries for specific functions data (sorted by percentage, highest first)
    fn get_sorted_measurements_for(
        functions: &FunctionsJson,
    ) -> Vec<(String, Vec<hotpath::MetricType>)> {
        use hotpath::MetricType;

        let mut entries: Vec<(String, Vec<MetricType>)> = functions
            .data
            .0
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();

        entries.sort_by(|(name_a, metrics_a), (name_b, metrics_b)| {
            let percent_a = metrics_a
                .iter()
                .find_map(|m| {
                    if let MetricType::Percentage(p) = m {
                        Some(*p)
                    } else {
                        None
                    }
                })
                .unwrap_or(0);

            let percent_b = metrics_b
                .iter()
                .find_map(|m| {
                    if let MetricType::Percentage(p) = m {
                        Some(*p)
                    } else {
                        None
                    }
                })
                .unwrap_or(0);

            percent_b.cmp(&percent_a).then_with(|| name_a.cmp(name_b))
        });

        entries
    }

    /// Get sorted entries (sorted by percentage, highest first)
    pub(crate) fn get_sorted_measurements(&self) -> Vec<(String, Vec<hotpath::MetricType>)> {
        let functions = self.active_functions();
        Self::get_sorted_measurements_for(functions)
    }

    /// Get sorted timing measurements
    pub(crate) fn get_timing_measurements(&self) -> Vec<(String, Vec<hotpath::MetricType>)> {
        Self::get_sorted_measurements_for(&self.timing_functions)
    }

    /// Get sorted memory measurements
    pub(crate) fn get_memory_measurements(&self) -> Vec<(String, Vec<hotpath::MetricType>)> {
        Self::get_sorted_measurements_for(&self.memory_functions)
    }

    pub(crate) fn selected_function_name(&self) -> Option<String> {
        let sorted_entries = self.get_sorted_measurements();
        let table_state = match self.selected_tab {
            SelectedTab::Timing => &self.timing_table_state,
            SelectedTab::Memory => &self.memory_table_state,
            _ => return None,
        };
        table_state
            .selected()
            .and_then(|idx| sorted_entries.get(idx).map(|(name, _)| name.clone()))
    }

    pub(crate) fn update_function_logs(&mut self, function_logs: FunctionLogsJson) {
        self.current_function_logs = Some(function_logs);
    }

    pub(crate) fn clear_function_logs(&mut self) {
        self.current_function_logs = None;
    }

    pub(crate) fn update_pinned_function(&mut self) {
        if self.show_function_logs {
            self.pinned_function = self.selected_function_name();
        }
    }

    pub(crate) fn logs_function_name(&self) -> Option<&str> {
        self.pinned_function.as_deref()
    }

    /// Fetch logs for pinned function if panel is open
    pub(crate) fn fetch_function_logs_if_open(&mut self, port: u16) {
        if self.show_function_logs {
            if let Some(function_name) = self.logs_function_name() {
                match self.selected_tab {
                    SelectedTab::Timing => {
                        match super::super::http::fetch_function_logs_timing(
                            &self.agent,
                            port,
                            function_name,
                        ) {
                            Ok(Some(function_logs)) => self.update_function_logs(function_logs),
                            Ok(None) => {
                                // Function not found, clear logs
                                self.clear_function_logs();
                            }
                            Err(_) => self.clear_function_logs(),
                        }
                    }
                    SelectedTab::Memory => {
                        match super::super::http::fetch_function_logs_alloc(
                            &self.agent,
                            port,
                            function_name,
                        ) {
                            Ok(Some(function_logs)) => self.update_function_logs(function_logs),
                            Ok(None) => {
                                // Feature not enabled, clear logs
                                self.clear_function_logs();
                            }
                            Err(_) => self.clear_function_logs(),
                        }
                    }
                    _ => {
                        // Other tabs don't support function logs
                        self.clear_function_logs();
                    }
                }
            }
        }
    }

    /// Update pinned function and fetch function logs if panel is open
    pub(crate) fn update_and_fetch_function_logs(&mut self, port: u16) {
        self.update_pinned_function();
        self.fetch_function_logs_if_open(port);
    }

    pub(crate) fn update_streams(&mut self, streams: StreamsJson) {
        // Capture the currently selected stream ID (not index!)
        let selected_stream_id = self
            .streams_table_state
            .selected()
            .and_then(|idx| self.streams.streams.get(idx))
            .map(|stat| stat.id);

        self.streams = streams;
        self.last_successful_fetch = Some(Instant::now());
        self.error_message = None;

        // Try to restore selection to the same stream ID
        if let Some(stream_id) = selected_stream_id {
            // Find the new index of the previously selected stream
            if let Some(new_idx) = self
                .streams
                .streams
                .iter()
                .position(|stat| stat.id == stream_id)
            {
                self.streams_table_state.select(Some(new_idx));
            } else {
                // Stream no longer exists, select the last one if available
                if !self.streams.streams.is_empty() {
                    self.streams_table_state
                        .select(Some(self.streams.streams.len() - 1));
                }
            }
        } else if let Some(selected) = self.streams_table_state.selected() {
            if selected >= self.streams.streams.len() && !self.streams.streams.is_empty() {
                self.streams_table_state
                    .select(Some(self.streams.streams.len() - 1));
            }
        }

        if self.show_stream_logs {
            self.refresh_stream_logs();
        }
    }

    pub(crate) fn update_threads(&mut self, threads: ThreadsJson) {
        // Capture the currently selected thread TID (not index!)
        let selected_thread_tid = self
            .threads_table_state
            .selected()
            .and_then(|idx| self.threads.threads.get(idx))
            .map(|stat| stat.os_tid);

        self.threads = threads;
        self.last_successful_fetch = Some(Instant::now());
        self.error_message = None;

        // Try to restore selection to the same thread TID
        if let Some(thread_tid) = selected_thread_tid {
            // Find the new index of the previously selected thread
            if let Some(new_idx) = self
                .threads
                .threads
                .iter()
                .position(|stat| stat.os_tid == thread_tid)
            {
                self.threads_table_state.select(Some(new_idx));
            } else {
                // Thread no longer exists, select the last one if available
                if !self.threads.threads.is_empty() {
                    self.threads_table_state
                        .select(Some(self.threads.threads.len() - 1));
                }
            }
        } else if let Some(selected) = self.threads_table_state.selected() {
            if selected >= self.threads.threads.len() && !self.threads.threads.is_empty() {
                self.threads_table_state
                    .select(Some(self.threads.threads.len() - 1));
            }
        }
    }

    pub(crate) fn refresh_stream_logs(&mut self) {
        if self.paused {
            return;
        }

        self.stream_logs = None;

        if let Some(selected) = self.streams_table_state.selected() {
            if !self.streams.streams.is_empty() && selected < self.streams.streams.len() {
                let stream_id = self.streams.streams[selected].id;
                if let Ok(logs) =
                    super::super::http::fetch_stream_logs(&self.agent, self.metrics_port, stream_id)
                {
                    self.stream_logs = Some(CachedStreamLogs { logs });

                    // Ensure logs table selection is valid
                    if let Some(ref cached_logs) = self.stream_logs {
                        let log_count = cached_logs.logs.logs.len();
                        if let Some(selected) = self.stream_logs_table_state.selected() {
                            if selected >= log_count && log_count > 0 {
                                self.stream_logs_table_state.select(Some(log_count - 1));
                            }
                        }
                    }
                }
            }
        }
    }

    pub(crate) fn refresh_data(&mut self) {
        match self.selected_tab {
            SelectedTab::Timing => {
                match super::super::http::fetch_functions_timing(&self.agent, self.metrics_port) {
                    Ok(metrics) => {
                        self.update_timing_metrics(metrics);
                    }
                    Err(e) => {
                        self.set_error(format!("{}", e));
                    }
                }
                self.fetch_function_logs_if_open(self.metrics_port);
            }
            SelectedTab::Memory => {
                match super::super::http::fetch_functions_alloc(&self.agent, self.metrics_port) {
                    Ok(Some(metrics)) => {
                        self.memory_available = true;
                        self.update_memory_metrics(metrics);
                    }
                    Ok(None) => {
                        self.memory_available = false;
                        self.set_error(
                            "Memory profiling not available - enable hotpath-alloc feature"
                                .to_string(),
                        );
                    }
                    Err(e) => {
                        self.set_error(format!("{}", e));
                    }
                }
                self.fetch_function_logs_if_open(self.metrics_port);
            }
            SelectedTab::Channels => {
                match super::super::http::fetch_channels(&self.agent, self.metrics_port) {
                    Ok(channels) => {
                        self.update_channels(channels);
                    }
                    Err(e) => {
                        self.set_error(format!("{}", e));
                    }
                }
            }
            SelectedTab::Streams => {
                match super::super::http::fetch_streams(&self.agent, self.metrics_port) {
                    Ok(streams) => {
                        self.update_streams(streams);
                    }
                    Err(e) => {
                        self.set_error(format!("{}", e));
                    }
                }
            }
            SelectedTab::Threads => {
                match super::super::http::fetch_threads(&self.agent, self.metrics_port) {
                    Ok(threads) => {
                        self.update_threads(threads);
                    }
                    Err(e) => {
                        self.set_error(format!("{}", e));
                    }
                }
            }
        }
        self.last_refresh = Instant::now();
    }
}
