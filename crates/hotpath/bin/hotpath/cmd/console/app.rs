//! TUI application state and main run loop

use crossbeam_channel::{Receiver, Sender};
use hotpath::formatted_output::{
    FormattedChannelLogs, FormattedChannelsJson, FormattedFunctionAllocLogsJson,
    FormattedFunctionTimingLogsJson, FormattedFunctionsJson, FormattedFutureCalls,
    FormattedFuturesJson, FormattedLogEntry, FormattedSentLogEntry, FormattedStreamLogs,
    FormattedStreamsJson, FormattedThreadsJson,
};
use ratatui::widgets::TableState;
use std::time::{Duration, Instant};

use crate::cmd::console::events::{AppEvent, DataRequest};

mod data;
mod keys;
mod state;

#[derive(Default, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SelectedTab {
    #[default]
    Timing,
    Memory,
    Futures,
    Channels,
    Streams,
    Threads,
}

impl SelectedTab {
    pub(crate) fn number(&self) -> u8 {
        match self {
            SelectedTab::Timing => 1,
            SelectedTab::Memory => 2,
            SelectedTab::Futures => 3,
            SelectedTab::Channels => 4,
            SelectedTab::Streams => 5,
            SelectedTab::Threads => 6,
        }
    }

    pub(crate) fn name(&self) -> &'static str {
        match self {
            SelectedTab::Timing => "Timing",
            SelectedTab::Memory => "Memory",
            SelectedTab::Futures => "Futures",
            SelectedTab::Channels => "Channels",
            SelectedTab::Streams => "Streams",
            SelectedTab::Threads => "Threads",
        }
    }

    pub(crate) fn is_functions_tab(&self) -> bool {
        matches!(self, SelectedTab::Timing | SelectedTab::Memory)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ChannelsFocus {
    Channels,
    Logs,
    Inspect,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StreamsFocus {
    Streams,
    Logs,
    Inspect,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FunctionsFocus {
    Functions,
    Logs,
    Inspect,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FuturesFocus {
    Futures,
    Calls,
    Inspect,
}

pub(crate) struct CachedChannelLogs {
    pub(crate) logs: FormattedChannelLogs,
}

#[derive(Debug, Clone)]
pub(crate) struct InspectedFunctionLog {
    pub(crate) invocation_index: usize,
    pub(crate) value: String,
    pub(crate) ago: String,
    pub(crate) thread_id: Option<u64>,
    pub(crate) result: Option<String>,
}

pub(crate) struct CachedStreamLogs {
    pub(crate) logs: FormattedStreamLogs,
}

pub(crate) struct App {
    pub(crate) timing_functions: FormattedFunctionsJson,
    pub(crate) memory_functions: FormattedFunctionsJson,
    pub(crate) memory_available: bool,
    pub(crate) channels: FormattedChannelsJson,
    pub(crate) streams: FormattedStreamsJson,

    pub(crate) timing_table_state: TableState,
    pub(crate) memory_table_state: TableState,
    pub(crate) channels_table_state: TableState,
    pub(crate) streams_table_state: TableState,
    pub(crate) selected_tab: SelectedTab,
    pub(crate) paused: bool,

    pub(crate) last_refresh: Instant,
    pub(crate) last_successful_fetch: Option<Instant>,
    pub(crate) error_message: Option<String>,

    pub(crate) function_logs_table_state: TableState,
    pub(crate) functions_focus: FunctionsFocus,
    pub(crate) show_function_logs: bool,
    pub(crate) current_timing_logs: Option<FormattedFunctionTimingLogsJson>,
    pub(crate) current_alloc_logs: Option<FormattedFunctionAllocLogsJson>,
    pub(crate) pinned_function: Option<String>,
    pub(crate) inspected_function_log: Option<InspectedFunctionLog>,

    pub(crate) request_tx: Sender<DataRequest>,
    pub(crate) event_rx: Receiver<AppEvent>,
    pub(crate) refresh_interval: Duration,
    pub(crate) metrics_host: String,
    exit: bool,

    pub(crate) loading_functions: bool,
    pub(crate) loading_channels: bool,
    pub(crate) loading_streams: bool,
    pub(crate) loading_threads: bool,
    pub(crate) loading_futures: bool,

    pub(crate) channel_logs_table_state: TableState,
    pub(crate) channels_focus: ChannelsFocus,
    pub(crate) show_logs: bool,
    pub(crate) channel_logs: Option<CachedChannelLogs>,
    pub(crate) inspected_channel_log: Option<FormattedSentLogEntry>,

    pub(crate) stream_logs_table_state: TableState,
    pub(crate) streams_focus: StreamsFocus,
    pub(crate) show_stream_logs: bool,
    pub(crate) stream_logs: Option<CachedStreamLogs>,
    pub(crate) inspected_stream_log: Option<FormattedLogEntry>,
    pub(crate) threads: FormattedThreadsJson,
    pub(crate) threads_table_state: TableState,

    pub(crate) futures: FormattedFuturesJson,
    pub(crate) futures_table_state: TableState,
    pub(crate) futures_focus: FuturesFocus,
    pub(crate) show_future_calls: bool,
    pub(crate) future_calls_table_state: TableState,
    pub(crate) future_calls: Option<FormattedFutureCalls>,
    pub(crate) inspected_future_call: Option<hotpath::formatted_output::FormattedFutureCall>,
}

fn empty_functions() -> FormattedFunctionsJson {
    FormattedFunctionsJson {
        profiling_mode: "timing".to_string(),
        total_elapsed: "0ns".to_string(),
        description: "Waiting for data...".to_string(),
        caller_name: "unknown".to_string(),
        percentiles: vec![95],
        data: Vec::new(),
    }
}

#[hotpath::measure_all]
impl App {
    pub(crate) fn new(metrics_host: &str, metrics_port: u16, refresh_interval_ms: u64) -> Self {
        let (request_tx, request_rx) = crossbeam_channel::unbounded();
        let (event_tx, event_rx) = crossbeam_channel::unbounded();

        let base_url = format!("{}:{}", metrics_host.trim_end_matches('/'), metrics_port);
        crate::cmd::console::http_worker::spawn_http_worker(
            request_rx,
            event_tx.clone(),
            base_url.clone(),
        );
        crate::cmd::console::input::spawn_input_reader(event_tx);

        Self {
            timing_functions: empty_functions(),
            memory_functions: empty_functions(),
            memory_available: true,
            channels: FormattedChannelsJson {
                current_elapsed: "0ns".to_string(),
                current_elapsed_ns: 0,
                channels: vec![],
            },
            streams: FormattedStreamsJson {
                current_elapsed: "0ns".to_string(),
                current_elapsed_ns: 0,
                streams: vec![],
            },
            timing_table_state: TableState::default().with_selected(0),
            memory_table_state: TableState::default().with_selected(0),
            channels_table_state: TableState::default().with_selected(0),
            streams_table_state: TableState::default().with_selected(0),
            selected_tab: SelectedTab::default(),
            paused: false,
            last_refresh: Instant::now(),
            last_successful_fetch: None,
            error_message: None,
            function_logs_table_state: TableState::default(),
            functions_focus: FunctionsFocus::Functions,
            show_function_logs: false,
            current_timing_logs: None,
            current_alloc_logs: None,
            pinned_function: None,
            inspected_function_log: None,
            request_tx,
            event_rx,
            refresh_interval: Duration::from_millis(refresh_interval_ms),
            metrics_host: base_url,
            exit: false,
            loading_functions: false,
            loading_channels: false,
            loading_streams: false,
            loading_threads: false,
            loading_futures: false,
            channel_logs_table_state: TableState::default(),
            channels_focus: ChannelsFocus::Channels,
            show_logs: false,
            channel_logs: None,
            inspected_channel_log: None,
            stream_logs_table_state: TableState::default(),
            streams_focus: StreamsFocus::Streams,
            show_stream_logs: false,
            stream_logs: None,
            inspected_stream_log: None,
            threads: FormattedThreadsJson {
                current_elapsed: "0ns".to_string(),
                current_elapsed_ns: 0,
                sample_interval_ms: 1000,
                threads: vec![],
                thread_count: 0,
                rss_bytes: None,
                rss_bytes_raw: None,
            },
            threads_table_state: TableState::default().with_selected(0),
            futures: FormattedFuturesJson {
                current_elapsed: "0ns".to_string(),
                current_elapsed_ns: 0,
                futures: vec![],
            },
            futures_table_state: TableState::default().with_selected(0),
            futures_focus: FuturesFocus::Futures,
            show_future_calls: false,
            future_calls_table_state: TableState::default(),
            future_calls: None,
            inspected_future_call: None,
        }
    }

    pub(crate) fn exit(&mut self) {
        self.exit = true;
    }

    pub(crate) fn active_functions(&self) -> &FormattedFunctionsJson {
        match self.selected_tab {
            SelectedTab::Timing => &self.timing_functions,
            SelectedTab::Memory => &self.memory_functions,
            _ => unreachable!("active_functions() called on non-functions tab"),
        }
    }

    pub(crate) fn active_table_state_mut(&mut self) -> &mut TableState {
        match self.selected_tab {
            SelectedTab::Timing => &mut self.timing_table_state,
            SelectedTab::Memory => &mut self.memory_table_state,
            SelectedTab::Channels => &mut self.channels_table_state,
            SelectedTab::Streams => &mut self.streams_table_state,
            SelectedTab::Threads => &mut self.threads_table_state,
            SelectedTab::Futures => &mut self.futures_table_state,
        }
    }

    pub(crate) fn run(
        &mut self,
        terminal: &mut ratatui::Terminal<ratatui::backend::CrosstermBackend<std::io::Stdout>>,
    ) -> std::io::Result<()> {
        use crossbeam_channel::select;

        self.request_refresh_for_current_tab();

        while !self.exit {
            terminal.draw(|frame| crate::cmd::console::views::render_ui(frame, self))?;

            select! {
                recv(self.event_rx) -> event => {
                    if let Ok(event) = event {
                        match event {
                            AppEvent::Key(key_code) => self.handle_key_event(key_code),
                            AppEvent::Data(response) => self.handle_data_response(response),
                        }
                    }
                }
                default(self.refresh_interval) => {
                    if !self.paused {
                        self.request_refresh_for_current_tab();
                    }
                }
            }
        }

        Ok(())
    }
}
