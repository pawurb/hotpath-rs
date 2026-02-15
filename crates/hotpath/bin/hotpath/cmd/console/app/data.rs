//! Data management - fetching, updating, and transforming functions/data flow

use crate::cmd::console::app::{App, DataFlowLogs, SelectedTab};
use crate::cmd::console::events::{DataRequest, DataResponse};
use hotpath::json::{
    DataFlowType, DebugEntryType, JsonChannelLogsList, JsonDataFlowList, JsonDebugList,
    JsonFunctionAllocLogsList, JsonFunctionEntry, JsonFunctionTimingLogsList, JsonFunctionsList,
    JsonFutureLogsList, JsonStreamLogsList, JsonThreadsList,
};
use std::time::Instant;
use tracing::{trace, warn};

#[hotpath::measure_all]
impl App {
    fn try_auto_expand_logs(&mut self) {
        if !self.auto_expand_logs {
            return;
        }
        match self.selected_tab {
            SelectedTab::Timing if !self.timing_functions.data.is_empty() => {
                self.auto_expand_logs = false;
                self.toggle_function_logs();
            }
            SelectedTab::Memory if !self.memory_functions.data.is_empty() => {
                self.auto_expand_logs = false;
                self.toggle_function_logs();
            }
            SelectedTab::DataFlow if !self.data_flow.entries.is_empty() => {
                self.auto_expand_logs = false;
                self.toggle_data_flow_logs();
            }
            SelectedTab::Debug if !self.debug_stats.is_empty() => {
                self.auto_expand_logs = false;
                self.toggle_debug_logs();
            }
            SelectedTab::Threads | SelectedTab::Runtime => {
                self.auto_expand_logs = false;
            }
            _ => {}
        }
    }

    pub(crate) fn update_timing_metrics(&mut self, metrics: JsonFunctionsList) {
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

        self.try_auto_expand_logs();
    }

    pub(crate) fn update_memory_metrics(&mut self, metrics: JsonFunctionsList) {
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

        self.try_auto_expand_logs();
    }

    pub(crate) fn set_error(&mut self, error: String) {
        self.error_message = Some(error);
    }

    #[hotpath::measure(log = true)]
    pub(crate) fn get_timing_measurements(&self) -> &[JsonFunctionEntry] {
        &self.timing_functions.data
    }

    #[hotpath::measure(log = true)]
    pub(crate) fn get_memory_measurements(&self) -> &[JsonFunctionEntry] {
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

    pub(crate) fn update_timing_logs(&mut self, logs: JsonFunctionTimingLogsList) {
        self.current_timing_logs = Some(logs);
    }

    pub(crate) fn update_alloc_logs(&mut self, logs: JsonFunctionAllocLogsList) {
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
                    _ => {}
                }
            }
        }
    }

    pub(crate) fn update_and_request_function_logs(&mut self) {
        self.update_pinned_function();
        self.request_function_logs_if_open();
    }

    // Data Flow methods

    pub(crate) fn update_data_flow(&mut self, data_flow: JsonDataFlowList) {
        let selected_id = self
            .data_flow_table_state
            .selected()
            .and_then(|idx| self.data_flow.entries.get(idx))
            .map(|entry| entry.id);

        self.data_flow = data_flow;
        self.last_successful_fetch = Some(Instant::now());
        self.error_message = None;

        if let Some(id) = selected_id {
            if let Some(new_idx) = self.data_flow.entries.iter().position(|e| e.id == id) {
                self.data_flow_table_state.select(Some(new_idx));
            } else if !self.data_flow.entries.is_empty() {
                self.data_flow_table_state
                    .select(Some(self.data_flow.entries.len() - 1));
            }
        } else if let Some(selected) = self.data_flow_table_state.selected() {
            if selected >= self.data_flow.entries.len() && !self.data_flow.entries.is_empty() {
                self.data_flow_table_state
                    .select(Some(self.data_flow.entries.len() - 1));
            }
        }

        if self.show_data_flow_logs {
            self.request_data_flow_logs();
        }

        self.try_auto_expand_logs();
    }

    pub(crate) fn request_data_flow_logs(&self) {
        if self.paused {
            return;
        }

        if let Some(selected) = self.data_flow_table_state.selected() {
            if let Some(entry) = self.data_flow.entries.get(selected) {
                let request = match entry.data_flow_type {
                    DataFlowType::Channel => DataRequest::FetchDataFlowChannelLogs(entry.id),
                    DataFlowType::Stream => DataRequest::FetchDataFlowStreamLogs(entry.id),
                    DataFlowType::Future => DataRequest::FetchDataFlowFutureLogs(entry.id),
                };
                let _ = self.request_tx.send(request);
            }
        }
    }

    pub(crate) fn handle_data_flow_channel_logs(&mut self, _id: u64, logs: JsonChannelLogsList) {
        self.data_flow_logs = Some(DataFlowLogs::Channel(logs));
        self.ensure_data_flow_logs_selection_valid();
    }

    pub(crate) fn handle_data_flow_stream_logs(&mut self, _id: u64, logs: JsonStreamLogsList) {
        self.data_flow_logs = Some(DataFlowLogs::Stream(logs));
        self.ensure_data_flow_logs_selection_valid();
    }

    pub(crate) fn handle_data_flow_future_logs(&mut self, _id: u64, calls: JsonFutureLogsList) {
        self.data_flow_logs = Some(DataFlowLogs::Future(calls));
        self.ensure_data_flow_logs_selection_valid();
    }

    fn ensure_data_flow_logs_selection_valid(&mut self) {
        let log_count = match &self.data_flow_logs {
            Some(DataFlowLogs::Channel(l)) => l.sent_logs.len(),
            Some(DataFlowLogs::Stream(l)) => l.logs.len(),
            Some(DataFlowLogs::Future(l)) => l.calls.len(),
            None => 0,
        };

        if let Some(selected) = self.data_flow_logs_table_state.selected() {
            if selected >= log_count && log_count > 0 {
                self.data_flow_logs_table_state.select(Some(log_count - 1));
            }
        }
    }

    // Threads methods

    pub(crate) fn update_threads(&mut self, threads: JsonThreadsList) {
        let selected_thread_tid = self
            .threads_table_state
            .selected()
            .and_then(|idx| self.threads.data.get(idx))
            .map(|stat| stat.os_tid);

        self.threads = threads;
        self.last_successful_fetch = Some(Instant::now());
        self.error_message = None;

        if let Some(thread_tid) = selected_thread_tid {
            if let Some(new_idx) = self
                .threads
                .data
                .iter()
                .position(|stat| stat.os_tid == thread_tid)
            {
                self.threads_table_state.select(Some(new_idx));
            } else if !self.threads.data.is_empty() {
                self.threads_table_state
                    .select(Some(self.threads.data.len() - 1));
            }
        } else if let Some(selected) = self.threads_table_state.selected() {
            if selected >= self.threads.data.len() && !self.threads.data.is_empty() {
                self.threads_table_state
                    .select(Some(self.threads.data.len() - 1));
            }
        }

        self.try_auto_expand_logs();
    }

    // Debug methods

    pub(crate) fn update_debug(&mut self, debug: JsonDebugList) {
        let selected_id = self
            .debug_table_state
            .selected()
            .and_then(|idx| self.debug_stats.get(idx))
            .map(|stat| stat.id);

        self.debug_stats = debug.entries;
        self.last_successful_fetch = Some(Instant::now());
        self.error_message = None;

        if let Some(id) = selected_id {
            if let Some(new_idx) = self.debug_stats.iter().position(|stat| stat.id == id) {
                self.debug_table_state.select(Some(new_idx));
            } else if !self.debug_stats.is_empty() {
                self.debug_table_state
                    .select(Some(self.debug_stats.len() - 1));
            }
        } else if let Some(selected) = self.debug_table_state.selected() {
            if selected >= self.debug_stats.len() && !self.debug_stats.is_empty() {
                self.debug_table_state
                    .select(Some(self.debug_stats.len() - 1));
            }
        }

        if self.show_debug_logs {
            self.request_debug_logs();
        }

        self.try_auto_expand_logs();
    }

    pub(crate) fn request_debug_logs(&self) {
        if self.paused {
            return;
        }

        if let Some(selected) = self.debug_table_state.selected() {
            if !self.debug_stats.is_empty() && selected < self.debug_stats.len() {
                let stat = &self.debug_stats[selected];
                let request = match stat.entry_type {
                    DebugEntryType::Dbg => DataRequest::FetchDebugDbgLogs(stat.id),
                    DebugEntryType::Val => DataRequest::FetchDebugValLogs(stat.id),
                    DebugEntryType::Gauge => DataRequest::FetchDebugGaugeLogs(stat.id),
                };
                let _ = self.request_tx.send(request);
            }
        }
    }

    pub(crate) fn handle_debug_logs(&mut self, logs: Vec<hotpath::json::JsonDebugLog>) {
        self.debug_logs = Some(logs);

        if let Some(ref cached_logs) = self.debug_logs {
            let log_count = cached_logs.len();
            if let Some(selected) = self.debug_logs_table_state.selected() {
                if selected >= log_count && log_count > 0 {
                    self.debug_logs_table_state.select(Some(log_count - 1));
                }
            }
        }
    }

    // Refresh and response handling

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
            SelectedTab::DataFlow => {
                self.loading_data_flow = true;
                DataRequest::RefreshDataFlow
            }
            SelectedTab::Threads => {
                self.loading_threads = true;
                DataRequest::RefreshThreads
            }
            SelectedTab::Debug => {
                self.loading_debug = true;
                DataRequest::RefreshDebug
            }
            SelectedTab::Runtime => {
                self.loading_runtime = true;
                DataRequest::RefreshTokioRuntime
            }
        };
        trace!("Requesting refresh for tab: {}", self.selected_tab.name());
        let _ = self.request_tx.send(request);
        let _ = self.request_tx.send(DataRequest::FetchProfilerStatus);
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
            DataResponse::DataFlow(data) => {
                trace!("Received data flow: {} entries", data.entries.len());
                self.loading_data_flow = false;
                self.update_data_flow(data);
            }
            DataResponse::DataFlowChannelLogs { id, logs } => {
                trace!(
                    "Received channel {} logs: {} sent, {} received",
                    id,
                    logs.sent_logs.len(),
                    logs.received_logs.len()
                );
                self.handle_data_flow_channel_logs(id, logs);
            }
            DataResponse::DataFlowStreamLogs { id, logs } => {
                trace!("Received stream {} logs: {} entries", id, logs.logs.len());
                self.handle_data_flow_stream_logs(id, logs);
            }
            DataResponse::DataFlowFutureLogs { id, calls } => {
                trace!(
                    "Received future {} calls: {} entries",
                    id,
                    calls.calls.len()
                );
                self.handle_data_flow_future_logs(id, calls);
            }
            DataResponse::DataFlowLogsNotFound { .. } => {
                self.data_flow_logs = None;
            }
            DataResponse::Threads(data) => {
                trace!("Received threads data: {} threads", data.data.len());
                self.loading_threads = false;
                self.update_threads(data);
            }
            DataResponse::Debug(data) => {
                trace!("Received debug data: {} entries", data.entries.len());
                self.loading_debug = false;
                self.update_debug(data);
            }
            DataResponse::DebugDbgLogs { id, logs } => {
                trace!("Received dbg logs for {}: {} entries", id, logs.len());
                self.handle_debug_logs(logs);
            }
            DataResponse::DebugValLogs { id, logs } => {
                trace!("Received val logs for {}: {} entries", id, logs.len());
                self.handle_debug_logs(logs);
            }
            DataResponse::DebugGaugeLogs { id, logs } => {
                trace!("Received gauge logs for {}: {} entries", id, logs.len());
                self.handle_debug_logs(logs);
            }
            DataResponse::DebugLogsNotFound { .. } => {
                self.debug_logs = None;
            }
            DataResponse::TokioRuntime(snapshot) => {
                trace!("Received tokio runtime snapshot");
                self.loading_runtime = false;
                self.tokio_runtime = Some(snapshot);
                self.last_successful_fetch = Some(Instant::now());
                self.error_message = None;
                self.try_auto_expand_logs();
            }
            DataResponse::ProfilerStatus(status) => {
                trace!("Received profiler status: uptime={}", status.uptime);
                self.program_uptime = Some(status.uptime);
            }
            DataResponse::Error(e) => {
                warn!("Data fetch error: {}", e);
                self.loading_functions = false;
                self.loading_data_flow = false;
                self.loading_threads = false;
                self.loading_debug = false;
                self.loading_runtime = false;
                self.set_error(e);
            }
        }
    }
}
