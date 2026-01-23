//! Data management - fetching, updating, and transforming functions/channels

use super::{App, CachedLogs, CachedStreamLogs, SelectedTab};
use crate::cmd::console::events::{DataRequest, DataResponse};
use hotpath::json::{
    FormattedChannelLogs, FormattedChannelsJson, FormattedFunctionAllocLogsJson,
    FormattedFunctionData, FormattedFunctionTimingLogsJson, FormattedFunctionsJson,
    FormattedFutureCalls, FormattedFuturesJson, FormattedStreamLogs, FormattedStreamsJson,
    FormattedThreadsJson,
};
use std::time::Instant;
use tracing::{trace, warn};

#[hotpath::measure_all]
impl App {
    pub(crate) fn update_timing_metrics(&mut self, metrics: FormattedFunctionsJson) {
        // Capture the currently selected function name (not index!)
        let selected_function_name = self.selected_function_name();

        self.timing_functions = metrics;
        self.last_successful_fetch = Some(Instant::now());
        self.error_message = None;

        let entries = &self.timing_functions.data;

        if let Some(function_name) = selected_function_name {
            // Find the new index of the previously selected function in sorted order
            if let Some(new_idx) = entries.iter().position(|f| f.name == function_name) {
                self.timing_table_state.select(Some(new_idx));
            } else {
                // Function no longer exists, select the last one
                if !entries.is_empty() {
                    self.timing_table_state.select(Some(entries.len() - 1));
                }
            }
        } else if let Some(selected) = self.timing_table_state.selected() {
            // Bound check: if current selection is now out of bounds
            if selected >= entries.len() && !entries.is_empty() {
                self.timing_table_state.select(Some(entries.len() - 1));
            }
        } else if !entries.is_empty() {
            // No selection yet, select first item
            self.timing_table_state.select(Some(0));
        }
    }

    pub(crate) fn update_memory_metrics(&mut self, metrics: FormattedFunctionsJson) {
        // Capture the currently selected function name (not index!)
        let selected_function_name = self.selected_function_name();

        self.memory_functions = metrics;
        self.last_successful_fetch = Some(Instant::now());
        self.error_message = None;

        let entries = &self.memory_functions.data;

        if let Some(function_name) = selected_function_name {
            // Find the new index of the previously selected function in sorted order
            if let Some(new_idx) = entries.iter().position(|f| f.name == function_name) {
                self.memory_table_state.select(Some(new_idx));
            } else {
                // Function no longer exists, select the last one
                if !entries.is_empty() {
                    self.memory_table_state.select(Some(entries.len() - 1));
                }
            }
        } else if let Some(selected) = self.memory_table_state.selected() {
            // Bound check: if current selection is now out of bounds
            if selected >= entries.len() && !entries.is_empty() {
                self.memory_table_state.select(Some(entries.len() - 1));
            }
        } else if !entries.is_empty() {
            // No selection yet, select first item
            self.memory_table_state.select(Some(0));
        }
    }

    pub(crate) fn set_error(&mut self, error: String) {
        self.error_message = Some(error);
    }

    pub(crate) fn update_channels(&mut self, channels: FormattedChannelsJson) {
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

    pub(crate) fn handle_channel_logs(&mut self, _channel_id: u64, logs: FormattedChannelLogs) {
        self.logs = Some(CachedLogs { logs });

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
    pub(crate) fn get_timing_measurements(&self) -> &[FormattedFunctionData] {
        &self.timing_functions.data
    }

    #[hotpath::measure(log = true)]
    pub(crate) fn get_memory_measurements(&self) -> &[FormattedFunctionData] {
        &self.memory_functions.data
    }

    #[hotpath::measure(log = true)]
    pub(crate) fn selected_function_name(&self) -> Option<String> {
        let (entries, table_state) = match self.selected_tab {
            SelectedTab::Timing => (self.get_timing_measurements(), &self.timing_table_state),
            SelectedTab::Memory => (self.get_memory_measurements(), &self.memory_table_state),
            _ => return None,
        };
        table_state
            .selected()
            .and_then(|idx| entries.get(idx).map(|f| f.name.clone()))
    }

    pub(crate) fn update_timing_logs(&mut self, logs: FormattedFunctionTimingLogsJson) {
        self.current_timing_logs = Some(logs);
    }

    pub(crate) fn update_alloc_logs(&mut self, logs: FormattedFunctionAllocLogsJson) {
        self.current_alloc_logs = Some(logs);
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

    pub(crate) fn update_streams(&mut self, streams: FormattedStreamsJson) {
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

    pub(crate) fn update_threads(&mut self, threads: FormattedThreadsJson) {
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

    pub(crate) fn handle_stream_logs(&mut self, _stream_id: u64, logs: FormattedStreamLogs) {
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
        trace!("Requesting refresh for tab: {}", self.selected_tab.name());
        let _ = self.request_tx.send(request);
        self.last_refresh = Instant::now();
    }

    pub(crate) fn handle_data_response(&mut self, response: DataResponse) {
        match response {
            DataResponse::FunctionsTiming(data) => {
                trace!("Received timing data: {} functions", data.data.len());
                self.loading_functions = false;
                self.update_timing_metrics(data);
                self.request_function_logs_if_open();
            }
            DataResponse::FunctionsAlloc(data) => {
                trace!("Received alloc data: {} functions", data.data.len());
                self.loading_functions = false;
                self.memory_available = true;
                self.update_memory_metrics(data);
                self.request_function_logs_if_open();
            }
            DataResponse::FunctionsAllocUnavailable => {
                trace!("Memory profiling unavailable");
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
                trace!("Received function timing logs: {} entries", logs.logs.len());
                self.update_timing_logs(logs);
            }
            DataResponse::FunctionLogsTimingNotFound(_) => {
                self.current_timing_logs = None;
            }
            DataResponse::FunctionLogsAlloc {
                function_name: _,
                logs,
            } => {
                trace!("Received function alloc logs: {} entries", logs.logs.len());
                self.update_alloc_logs(logs);
            }
            DataResponse::FunctionLogsAllocNotFound(_) => {
                self.current_alloc_logs = None;
            }
            DataResponse::Channels(data) => {
                trace!("Received channels data: {} channels", data.channels.len());
                self.loading_channels = false;
                self.update_channels(data);
            }
            DataResponse::ChannelLogs { channel_id, logs } => {
                trace!(
                    "Received channel {} logs: {} sent, {} received",
                    channel_id,
                    logs.sent_logs.len(),
                    logs.received_logs.len()
                );
                self.handle_channel_logs(channel_id, logs);
            }
            DataResponse::Streams(data) => {
                trace!("Received streams data: {} streams", data.streams.len());
                self.loading_streams = false;
                self.update_streams(data);
            }
            DataResponse::StreamLogs { stream_id, logs } => {
                trace!(
                    "Received stream {} logs: {} entries",
                    stream_id,
                    logs.logs.len()
                );
                self.handle_stream_logs(stream_id, logs);
            }
            DataResponse::Threads(data) => {
                trace!("Received threads data: {} threads", data.threads.len());
                self.loading_threads = false;
                self.update_threads(data);
            }
            DataResponse::Futures(data) => {
                trace!("Received futures data: {} futures", data.futures.len());
                self.loading_futures = false;
                self.update_futures(data);
            }
            DataResponse::FutureCalls { future_id, calls } => {
                trace!(
                    "Received future {} calls: {} entries",
                    future_id,
                    calls.calls.len()
                );
                self.handle_future_calls(future_id, calls);
            }
            DataResponse::Error(e) => {
                warn!("Data fetch error: {}", e);
                self.loading_functions = false;
                self.loading_channels = false;
                self.loading_streams = false;
                self.loading_threads = false;
                self.loading_futures = false;
                self.set_error(e);
            }
        }
    }

    pub(crate) fn update_futures(&mut self, futures: FormattedFuturesJson) {
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

    pub(crate) fn handle_future_calls(&mut self, _future_id: u64, calls: FormattedFutureCalls) {
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
