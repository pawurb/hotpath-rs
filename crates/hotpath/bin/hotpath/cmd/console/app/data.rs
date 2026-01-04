//! Data management - fetching, updating, and transforming functions/channels

use super::{App, CachedLogs, CachedStreamLogs, SelectedTab};
use crate::cmd::console::events::{DataRequest, DataResponse};
use hotpath::json::{
    ChannelLogs, FunctionLogsJson, FunctionsJson, FutureCalls, FuturesJson as FuturesJsonData,
    StreamLogs, StreamsJson, ThreadsJson,
};
use std::collections::HashMap;
use std::time::Instant;

#[hotpath::measure_all]
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

    pub(crate) fn update_channels(&mut self, channels: hotpath::json::ChannelsJson) {
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
            self.request_channel_logs();
        }
    }

    pub(crate) fn request_channel_logs(&self) {
        if self.paused {
            return;
        }

        if let Some(selected) = self.channels_table_state.selected() {
            if !self.channels.channels.is_empty() && selected < self.channels.channels.len() {
                let channel_id = self.channels.channels[selected].id;
                let _ = self
                    .request_tx
                    .send(DataRequest::FetchChannelLogs(channel_id));
            }
        }
    }

    pub(crate) fn handle_channel_logs(&mut self, _channel_id: u64, logs: ChannelLogs) {
        let received_map: HashMap<u64, hotpath::json::LogEntry> = logs
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

    #[hotpath::measure(log = true)]
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

    #[hotpath::measure(log = true)]
    pub(crate) fn get_sorted_measurements(&self) -> Vec<(String, Vec<hotpath::MetricType>)> {
        let functions = self.active_functions();
        Self::get_sorted_measurements_for(functions)
    }

    #[hotpath::measure(log = true)]
    pub(crate) fn get_timing_measurements(&self) -> Vec<(String, Vec<hotpath::MetricType>)> {
        Self::get_sorted_measurements_for(&self.timing_functions)
    }

    #[hotpath::measure(log = true)]
    pub(crate) fn get_memory_measurements(&self) -> Vec<(String, Vec<hotpath::MetricType>)> {
        Self::get_sorted_measurements_for(&self.memory_functions)
    }

    #[hotpath::measure(log = true)]
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

    pub(crate) fn request_function_logs_if_open(&self) {
        if self.show_function_logs {
            if let Some(function_name) = self.logs_function_name() {
                match self.selected_tab {
                    SelectedTab::Timing => {
                        let _ = self.request_tx.send(DataRequest::FetchFunctionLogsTiming(
                            function_name.to_string(),
                        ));
                    }
                    SelectedTab::Memory => {
                        let _ = self.request_tx.send(DataRequest::FetchFunctionLogsAlloc(
                            function_name.to_string(),
                        ));
                    }
                    _ => {
                        // Other tabs don't support function logs
                    }
                }
            }
        }
    }

    pub(crate) fn update_and_request_function_logs(&mut self) {
        self.update_pinned_function();
        self.request_function_logs_if_open();
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
            self.request_stream_logs();
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

    pub(crate) fn request_stream_logs(&self) {
        if self.paused {
            return;
        }

        if let Some(selected) = self.streams_table_state.selected() {
            if !self.streams.streams.is_empty() && selected < self.streams.streams.len() {
                let stream_id = self.streams.streams[selected].id;
                let _ = self
                    .request_tx
                    .send(DataRequest::FetchStreamLogs(stream_id));
            }
        }
    }

    pub(crate) fn handle_stream_logs(&mut self, _stream_id: u64, logs: StreamLogs) {
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

    pub(crate) fn request_refresh_for_current_tab(&mut self) {
        let request = match self.selected_tab {
            SelectedTab::Timing => {
                self.loading_functions = true;
                DataRequest::RefreshTiming
            }
            SelectedTab::Memory => {
                self.loading_functions = true;
                DataRequest::RefreshMemory
            }
            SelectedTab::Channels => {
                self.loading_channels = true;
                DataRequest::RefreshChannels
            }
            SelectedTab::Streams => {
                self.loading_streams = true;
                DataRequest::RefreshStreams
            }
            SelectedTab::Threads => {
                self.loading_threads = true;
                DataRequest::RefreshThreads
            }
            SelectedTab::Futures => {
                self.loading_futures = true;
                DataRequest::RefreshFutures
            }
        };
        let _ = self.request_tx.send(request);
        self.last_refresh = Instant::now();
    }

    pub(crate) fn handle_data_response(&mut self, response: DataResponse) {
        match response {
            DataResponse::FunctionsTiming(data) => {
                self.loading_functions = false;
                self.update_timing_metrics(data);
                self.request_function_logs_if_open();
            }
            DataResponse::FunctionsAlloc(data) => {
                self.loading_functions = false;
                self.memory_available = true;
                self.update_memory_metrics(data);
                self.request_function_logs_if_open();
            }
            DataResponse::FunctionsAllocUnavailable => {
                self.loading_functions = false;
                self.memory_available = false;
                self.set_error(
                    "Memory profiling not available - enable hotpath-alloc feature".to_string(),
                );
            }
            DataResponse::FunctionLogsTiming {
                function_name: _,
                logs,
            } => {
                self.update_function_logs(logs);
            }
            DataResponse::FunctionLogsTimingNotFound(_) => {
                self.clear_function_logs();
            }
            DataResponse::FunctionLogsAlloc {
                function_name: _,
                logs,
            } => {
                self.update_function_logs(logs);
            }
            DataResponse::FunctionLogsAllocNotFound(_) => {
                self.clear_function_logs();
            }
            DataResponse::Channels(data) => {
                self.loading_channels = false;
                self.update_channels(data);
            }
            DataResponse::ChannelLogs { channel_id, logs } => {
                self.handle_channel_logs(channel_id, logs);
            }
            DataResponse::Streams(data) => {
                self.loading_streams = false;
                self.update_streams(data);
            }
            DataResponse::StreamLogs { stream_id, logs } => {
                self.handle_stream_logs(stream_id, logs);
            }
            DataResponse::Threads(data) => {
                self.loading_threads = false;
                self.update_threads(data);
            }
            DataResponse::Futures(data) => {
                self.loading_futures = false;
                self.update_futures(data);
            }
            DataResponse::FutureCalls { future_id, calls } => {
                self.handle_future_calls(future_id, calls);
            }
            DataResponse::Error(e) => {
                self.loading_functions = false;
                self.loading_channels = false;
                self.loading_streams = false;
                self.loading_threads = false;
                self.loading_futures = false;
                self.set_error(e);
            }
        }
    }

    pub(crate) fn update_futures(&mut self, futures: FuturesJsonData) {
        // Capture the currently selected future ID (not index!)
        let selected_future_id = self
            .futures_table_state
            .selected()
            .and_then(|idx| self.futures.futures.get(idx))
            .map(|stat| stat.id);

        self.futures = futures;
        self.last_successful_fetch = Some(Instant::now());
        self.error_message = None;

        // Try to restore selection to the same future ID
        if let Some(future_id) = selected_future_id {
            // Find the new index of the previously selected future
            if let Some(new_idx) = self
                .futures
                .futures
                .iter()
                .position(|stat| stat.id == future_id)
            {
                self.futures_table_state.select(Some(new_idx));
            } else {
                // Future no longer exists, select the last one if available
                if !self.futures.futures.is_empty() {
                    self.futures_table_state
                        .select(Some(self.futures.futures.len() - 1));
                }
            }
        } else if let Some(selected) = self.futures_table_state.selected() {
            if selected >= self.futures.futures.len() && !self.futures.futures.is_empty() {
                self.futures_table_state
                    .select(Some(self.futures.futures.len() - 1));
            }
        }

        if self.show_future_calls {
            self.request_future_calls();
        }
    }

    pub(crate) fn request_future_calls(&self) {
        if self.paused {
            return;
        }

        if let Some(selected) = self.futures_table_state.selected() {
            if !self.futures.futures.is_empty() && selected < self.futures.futures.len() {
                let future_id = self.futures.futures[selected].id;
                let _ = self
                    .request_tx
                    .send(DataRequest::FetchFutureCalls(future_id));
            }
        }
    }

    pub(crate) fn handle_future_calls(&mut self, _future_id: u64, calls: FutureCalls) {
        self.future_calls = Some(calls);

        // Ensure calls table selection is valid
        if let Some(ref future_calls) = self.future_calls {
            let call_count = future_calls.calls.len();
            if let Some(selected) = self.future_calls_table_state.selected() {
                if selected >= call_count && call_count > 0 {
                    self.future_calls_table_state.select(Some(call_count - 1));
                }
            }
        }
    }
}
