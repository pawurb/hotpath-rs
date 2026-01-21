//! Data management - fetching, updating, and transforming functions/channels

use crate::cmd::console::app::{
    CachedChannelLogs, CachedStreamLogs, InspectedFunctionLog, SelectedTab,
};
use crate::cmd::console::events::{DataRequest, DataResponse};
use hotpath::formatted_output::{
    FormattedChannelLogs, FormattedChannelsJson, FormattedFunctionAllocLogsJson,
    FormattedFunctionTimingLogsJson, FormattedFunctionsJson, FormattedFutureCalls,
    FormattedFuturesJson, FormattedStreamLogs, FormattedStreamsJson, FormattedThreadsJson,
};
use std::time::Instant;
use tracing::{trace, warn};

use super::App;

#[hotpath::measure_all]
impl App {
    pub(crate) fn update_timing_metrics(&mut self, metrics: FormattedFunctionsJson) {
        let selected_function_name = self.selected_function_name();

        self.timing_functions = metrics;
        self.last_successful_fetch = Some(Instant::now());
        self.error_message = None;

        let entries = &self.timing_functions.data;

        if let Some(function_name) = selected_function_name {
            if let Some(new_idx) = entries.iter().position(|f| f.name == function_name) {
                self.timing_table_state.select(Some(new_idx));
            } else if !entries.is_empty() {
                self.timing_table_state.select(Some(entries.len() - 1));
            }
        } else if let Some(selected) = self.timing_table_state.selected() {
            if selected >= entries.len() && !entries.is_empty() {
                self.timing_table_state.select(Some(entries.len() - 1));
            }
        } else if !entries.is_empty() {
            self.timing_table_state.select(Some(0));
        }
    }

    pub(crate) fn update_memory_metrics(&mut self, metrics: FormattedFunctionsJson) {
        let selected_function_name = self.selected_function_name();

        self.memory_functions = metrics;
        self.last_successful_fetch = Some(Instant::now());
        self.error_message = None;

        let entries = &self.memory_functions.data;

        if let Some(function_name) = selected_function_name {
            if let Some(new_idx) = entries.iter().position(|f| f.name == function_name) {
                self.memory_table_state.select(Some(new_idx));
            } else if !entries.is_empty() {
                self.memory_table_state.select(Some(entries.len() - 1));
            }
        } else if let Some(selected) = self.memory_table_state.selected() {
            if selected >= entries.len() && !entries.is_empty() {
                self.memory_table_state.select(Some(entries.len() - 1));
            }
        } else if !entries.is_empty() {
            self.memory_table_state.select(Some(0));
        }
    }

    pub(crate) fn set_error(&mut self, error: String) {
        self.error_message = Some(error);
    }

    pub(crate) fn update_channels(&mut self, channels: FormattedChannelsJson) {
        let selected_channel_id = self
            .channels_table_state
            .selected()
            .and_then(|idx| self.channels.channels.get(idx))
            .map(|stat| stat.id);

        self.channels = channels;
        self.last_successful_fetch = Some(Instant::now());
        self.error_message = None;

        if let Some(channel_id) = selected_channel_id {
            if let Some(new_idx) = self
                .channels
                .channels
                .iter()
                .position(|stat| stat.id == channel_id)
            {
                self.channels_table_state.select(Some(new_idx));
            } else if !self.channels.channels.is_empty() {
                self.channels_table_state
                    .select(Some(self.channels.channels.len() - 1));
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
        self.channel_logs = Some(CachedChannelLogs { logs });

        if let Some(ref cached_logs) = self.channel_logs {
            let log_count = cached_logs.logs.sent_logs.len();
            if let Some(selected) = self.channel_logs_table_state.selected() {
                if selected >= log_count && log_count > 0 {
                    self.channel_logs_table_state.select(Some(log_count - 1));
                }
            }
        }
    }

    #[hotpath::measure(log = true)]
    pub(crate) fn selected_function_name(&self) -> Option<String> {
        let (entries, table_state) = match self.selected_tab {
            SelectedTab::Timing => (&self.timing_functions.data, &self.timing_table_state),
            SelectedTab::Memory => (&self.memory_functions.data, &self.memory_table_state),
            _ => return None,
        };
        table_state
            .selected()
            .and_then(|idx| entries.get(idx).map(|f| f.name.clone()))
    }

    pub(crate) fn update_timing_logs(&mut self, function_logs: FormattedFunctionTimingLogsJson) {
        self.current_timing_logs = Some(function_logs);
    }

    pub(crate) fn update_alloc_logs(&mut self, function_logs: FormattedFunctionAllocLogsJson) {
        self.current_alloc_logs = Some(function_logs);
    }

    pub(crate) fn clear_function_logs(&mut self) {
        self.current_timing_logs = None;
        self.current_alloc_logs = None;
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
                    _ => {}
                }
            }
        }
    }

    pub(crate) fn update_and_request_function_logs(&mut self) {
        self.update_pinned_function();
        self.request_function_logs_if_open();
    }

    pub(crate) fn update_streams(&mut self, streams: FormattedStreamsJson) {
        let selected_stream_id = self
            .streams_table_state
            .selected()
            .and_then(|idx| self.streams.streams.get(idx))
            .map(|stat| stat.id);

        self.streams = streams;
        self.last_successful_fetch = Some(Instant::now());
        self.error_message = None;

        if let Some(stream_id) = selected_stream_id {
            if let Some(new_idx) = self
                .streams
                .streams
                .iter()
                .position(|stat| stat.id == stream_id)
            {
                self.streams_table_state.select(Some(new_idx));
            } else if !self.streams.streams.is_empty() {
                self.streams_table_state
                    .select(Some(self.streams.streams.len() - 1));
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
        let selected_thread_tid = self
            .threads_table_state
            .selected()
            .and_then(|idx| self.threads.threads.get(idx))
            .map(|stat| stat.os_tid);

        self.threads = threads;
        self.last_successful_fetch = Some(Instant::now());
        self.error_message = None;

        if let Some(thread_tid) = selected_thread_tid {
            if let Some(new_idx) = self
                .threads
                .threads
                .iter()
                .position(|stat| stat.os_tid == thread_tid)
            {
                self.threads_table_state.select(Some(new_idx));
            } else if !self.threads.threads.is_empty() {
                self.threads_table_state
                    .select(Some(self.threads.threads.len() - 1));
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
                self.clear_function_logs();
            }
            DataResponse::FunctionLogsAlloc {
                function_name: _,
                logs,
            } => {
                trace!("Received function alloc logs: {} entries", logs.logs.len());
                self.update_alloc_logs(logs);
            }
            DataResponse::FunctionLogsAllocNotFound(_) => {
                self.clear_function_logs();
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
        let selected_future_id = self
            .futures_table_state
            .selected()
            .and_then(|idx| self.futures.futures.get(idx))
            .map(|stat| stat.id);

        self.futures = futures;
        self.last_successful_fetch = Some(Instant::now());
        self.error_message = None;

        if let Some(future_id) = selected_future_id {
            if let Some(new_idx) = self
                .futures
                .futures
                .iter()
                .position(|stat| stat.id == future_id)
            {
                self.futures_table_state.select(Some(new_idx));
            } else if !self.futures.futures.is_empty() {
                self.futures_table_state
                    .select(Some(self.futures.futures.len() - 1));
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

        if let Some(ref future_calls) = self.future_calls {
            let call_count = future_calls.calls.len();
            if let Some(selected) = self.future_calls_table_state.selected() {
                if selected >= call_count && call_count > 0 {
                    self.future_calls_table_state.select(Some(call_count - 1));
                }
            }
        }
    }

    pub(crate) fn get_inspected_function_log(&self) -> Option<InspectedFunctionLog> {
        let selected_idx = self.function_logs_table_state.selected()?;

        match self.selected_tab {
            SelectedTab::Timing => {
                let logs = self.current_timing_logs.as_ref()?;
                let entry = logs.logs.get(selected_idx)?;
                Some(InspectedFunctionLog {
                    invocation_index: entry.invocation,
                    value: entry.duration.clone(),
                    ago: entry.ago.clone(),
                    thread_id: entry.thread_id,
                    result: entry.result.clone(),
                })
            }
            SelectedTab::Memory => {
                let logs = self.current_alloc_logs.as_ref()?;
                let entry = logs.logs.get(selected_idx)?;
                Some(InspectedFunctionLog {
                    invocation_index: entry.invocation,
                    value: entry.bytes.clone(),
                    ago: entry.ago.clone(),
                    thread_id: entry.thread_id,
                    result: entry.result.clone(),
                })
            }
            _ => None,
        }
    }
}
