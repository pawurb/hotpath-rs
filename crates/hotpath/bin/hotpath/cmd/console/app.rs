//! TUI application state and main run loop

use crossbeam_channel::{Receiver, Sender};
use hotpath::json::{
    ChannelLogs, ChannelsJson, FunctionLogsJson, FunctionsJson, FutureCall, FutureCalls,
    FuturesJson as FuturesJsonData, LogEntry, StreamLogs, StreamsJson, ThreadsJson,
};
use ratatui::widgets::TableState;
use std::collections::HashMap;
use std::time::{Duration, Instant};

use super::events::{AppEvent, DataRequest};

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

/// Represents which UI component has focus in the Channels tab
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ChannelsFocus {
    Channels,
    Logs,
    Inspect,
}

/// Represents which UI component has focus in the Streams tab
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StreamsFocus {
    Streams,
    Logs,
    Inspect,
}

/// Represents which UI component has focus in the Functions tab
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FunctionsFocus {
    Functions,
    Logs,
    Inspect,
}

/// Represents which UI component has focus in the Futures tab
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FuturesFocus {
    Futures,
    Calls,
    Inspect,
}

/// Cached logs with a lookup map for received entries
pub(crate) struct CachedLogs {
    pub(crate) logs: ChannelLogs,
    pub(crate) received_map: HashMap<u64, LogEntry>,
}

/// Inspected function log entry for the inspect popup
#[derive(Debug, Clone)]
pub(crate) struct InspectedFunctionLog {
    /// Invocation index (1-indexed, most recent first)
    pub(crate) invocation_index: usize,
    /// Measured value (duration in ns for timing, bytes for memory)
    pub(crate) value: Option<u64>,
    /// Timestamp when the measurement was taken (nanoseconds since start)
    pub(crate) elapsed_nanos: u64,
    /// Allocation count (only for memory mode)
    pub(crate) alloc_count: Option<u64>,
    /// Thread ID where the function was executed
    pub(crate) tid: Option<u64>,
    /// Debug representation of the return value (when log = true)
    pub(crate) result: Option<String>,
}

pub(crate) struct CachedStreamLogs {
    pub(crate) logs: StreamLogs,
}

pub(crate) struct App {
    pub(crate) timing_functions: FunctionsJson,
    pub(crate) memory_functions: FunctionsJson,
    pub(crate) memory_available: bool,
    pub(crate) channels: ChannelsJson,
    pub(crate) streams: StreamsJson,

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
    pub(crate) current_function_logs: Option<FunctionLogsJson>,
    pub(crate) pinned_function: Option<String>,
    pub(crate) inspected_function_log: Option<InspectedFunctionLog>,

    pub(crate) request_tx: Sender<DataRequest>,
    pub(crate) event_rx: Receiver<AppEvent>,
    pub(crate) refresh_interval: Duration,
    pub(crate) metrics_port: u16,
    exit: bool,

    pub(crate) loading_functions: bool,
    pub(crate) loading_channels: bool,
    pub(crate) loading_streams: bool,
    pub(crate) loading_threads: bool,
    pub(crate) loading_futures: bool,

    pub(crate) channel_logs_table_state: TableState,
    pub(crate) channels_focus: ChannelsFocus,
    pub(crate) show_logs: bool,
    pub(crate) logs: Option<CachedLogs>,
    pub(crate) inspected_log: Option<LogEntry>,

    pub(crate) stream_logs_table_state: TableState,
    pub(crate) streams_focus: StreamsFocus,
    pub(crate) show_stream_logs: bool,
    pub(crate) stream_logs: Option<CachedStreamLogs>,
    pub(crate) inspected_stream_log: Option<LogEntry>,
    pub(crate) threads: ThreadsJson,
    pub(crate) threads_table_state: TableState,

    pub(crate) futures: FuturesJsonData,
    pub(crate) futures_table_state: TableState,
    pub(crate) futures_focus: FuturesFocus,
    pub(crate) show_future_calls: bool,
    pub(crate) future_calls_table_state: TableState,
    pub(crate) future_calls: Option<FutureCalls>,
    pub(crate) inspected_future_call: Option<FutureCall>,
}

#[hotpath::measure_all]
impl App {
    pub(crate) fn new(metrics_port: u16, refresh_interval_ms: u64) -> Self {
        let (request_tx, request_rx) = crossbeam_channel::unbounded();
        let (event_tx, event_rx) = crossbeam_channel::unbounded();

        super::http_worker::spawn_http_worker(request_rx, event_tx.clone(), metrics_port);
        super::input::spawn_input_reader(event_tx);

        let empty_functions = FunctionsJson {
            hotpath_profiling_mode: hotpath::ProfilingMode::Timing,
            total_elapsed: 0,
            description: "Waiting for data...".to_string(),
            caller_name: "unknown".to_string(),
            percentiles: vec![95],
            data: hotpath::FunctionsDataJson(std::collections::HashMap::new()),
        };

        Self {
            timing_functions: empty_functions.clone(),
            memory_functions: empty_functions,
            memory_available: true,
            channels: hotpath::json::ChannelsJson {
                current_elapsed_ns: 0,
                channels: vec![],
            },
            streams: StreamsJson {
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
            current_function_logs: None,
            pinned_function: None,
            inspected_function_log: None,
            request_tx,
            event_rx,
            refresh_interval: Duration::from_millis(refresh_interval_ms),
            metrics_port,
            exit: false,
            loading_functions: false,
            loading_channels: false,
            loading_streams: false,
            loading_threads: false,
            loading_futures: false,
            channel_logs_table_state: TableState::default(),
            channels_focus: ChannelsFocus::Channels,
            show_logs: false,
            logs: None,
            inspected_log: None,
            stream_logs_table_state: TableState::default(),
            streams_focus: StreamsFocus::Streams,
            show_stream_logs: false,
            stream_logs: None,
            inspected_stream_log: None,
            threads: ThreadsJson {
                current_elapsed_ns: 0,
                sample_interval_ms: 1000,
                threads: vec![],
                thread_count: 0,
                rss_bytes: None,
            },
            threads_table_state: TableState::default().with_selected(0),
            futures: FuturesJsonData {
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

    pub(crate) fn active_functions(&self) -> &FunctionsJson {
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
            terminal.draw(|frame| super::views::render_ui(frame, self))?;

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
