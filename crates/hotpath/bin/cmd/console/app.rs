//! TUI application state and main run loop
//!
//! The App struct manages all TUI state including metrics, channels, UI state,
//! and user interactions. Implementation is split across modules:
//! - `state`: Navigation and selection logic
//! - `data`: Data fetching and updates
//! - `keys`: Keyboard input handling

use hotpath::channels::{ChannelLogs, LogEntry};
use hotpath::streams::{StreamLogs, StreamsJson};
use hotpath::{channels::ChannelsJson, FunctionLogsJson, FunctionsJson};
use ratatui::widgets::TableState;
use std::collections::HashMap;
use std::time::{Duration, Instant};

// Helper modules containing App implementation
mod data;
mod keys;
mod state;

#[derive(Default, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SelectedTab {
    #[default]
    Timing,
    Memory,
    Channels,
    Streams,
}

impl SelectedTab {
    pub(crate) fn number(&self) -> u8 {
        match self {
            SelectedTab::Timing => 1,
            SelectedTab::Memory => 2,
            SelectedTab::Channels => 3,
            SelectedTab::Streams => 4,
        }
    }

    pub(crate) fn name(&self) -> &'static str {
        match self {
            SelectedTab::Timing => "Timing",
            SelectedTab::Memory => "Memory",
            SelectedTab::Channels => "Channels",
            SelectedTab::Streams => "Streams",
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
}

/// Cached logs with a lookup map for received entries
pub(crate) struct CachedLogs {
    pub(crate) logs: ChannelLogs,
    pub(crate) received_map: HashMap<u64, LogEntry>,
}

pub(crate) struct CachedStreamLogs {
    pub(crate) logs: StreamLogs,
}

/// Main TUI application state
///
/// This struct manages all application state including:
/// - Functions data and channels data from the profiled application
/// - UI state (selected tab, table selections, focus)
/// - HTTP client for fetching data from the metrics server
/// - Error handling and connection status
///
/// The implementation is split across multiple modules to improve maintainability.
pub(crate) struct App {
    // Data from profiled application
    /// Timing functions data (from /functions_timing endpoint)
    pub(crate) timing_functions: FunctionsJson,
    /// Memory functions data (from /functions_alloc endpoint)
    pub(crate) memory_functions: FunctionsJson,
    /// Whether memory profiling is available (hotpath-alloc feature enabled)
    pub(crate) memory_available: bool,
    /// Current channels data
    pub(crate) channels: ChannelsJson,
    /// Current streams data
    pub(crate) streams: StreamsJson,

    // UI state - navigation and selection per tab
    /// Selection state for timing tab table
    pub(crate) timing_table_state: TableState,
    /// Selection state for memory tab table
    pub(crate) memory_table_state: TableState,
    /// Selection state for channels tab table
    pub(crate) channels_table_state: TableState,
    /// Selection state for streams tab table
    pub(crate) streams_table_state: TableState,
    /// Currently selected tab
    pub(crate) selected_tab: SelectedTab,
    /// Whether automatic refresh is paused
    pub(crate) paused: bool,

    // Timing and status
    /// Last time data was refreshed
    pub(crate) last_refresh: Instant,
    /// Last successful data fetch (for connection status)
    pub(crate) last_successful_fetch: Option<Instant>,
    /// Current error message to display, if any
    pub(crate) error_message: Option<String>,

    // Function logs panel (Functions tab)
    /// Selection state for function logs table
    pub(crate) function_logs_table_state: TableState,
    /// Which component has focus in Functions tab
    pub(crate) functions_focus: FunctionsFocus,
    /// Whether function logs panel is visible
    pub(crate) show_function_logs: bool,
    /// Current function logs data for selected function
    pub(crate) current_function_logs: Option<FunctionLogsJson>,
    /// Function pinned for logs display
    pub(crate) pinned_function: Option<String>,

    // HTTP client and configuration
    /// HTTP client for fetching data from metrics server
    pub(crate) agent: ureq::Agent,
    /// Port where metrics HTTP server is running
    pub(crate) metrics_port: u16,
    /// Whether the application should exit
    exit: bool,

    // Channels tab specific state
    /// Selection state for logs table
    pub(crate) channel_logs_table_state: TableState,
    /// Which component has focus in Channels tab
    pub(crate) channels_focus: ChannelsFocus,
    /// Whether logs panel is visible
    pub(crate) show_logs: bool,
    /// Cached logs data for selected channel
    pub(crate) logs: Option<CachedLogs>,
    /// Log entry being inspected in popup
    pub(crate) inspected_log: Option<LogEntry>,

    // Streams tab specific state
    /// Selection state for stream logs table
    pub(crate) stream_logs_table_state: TableState,
    /// Which component has focus in Streams tab
    pub(crate) streams_focus: StreamsFocus,
    /// Whether stream logs panel is visible
    pub(crate) show_stream_logs: bool,
    /// Cached logs data for selected stream
    pub(crate) stream_logs: Option<CachedStreamLogs>,
    /// Stream log entry being inspected in popup
    pub(crate) inspected_stream_log: Option<LogEntry>,
}

#[cfg_attr(feature = "hotpath", hotpath::measure_all)]
impl App {
    /// Create a new App instance
    pub(crate) fn new(metrics_port: u16) -> Self {
        let config = ureq::Agent::config_builder()
            .timeout_global(Some(super::constants::http_timeout()))
            .build();
        let agent: ureq::Agent = config.into();

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
            memory_available: true, // Assume available until we know otherwise
            channels: hotpath::channels::ChannelsJson {
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
            agent,
            metrics_port,
            exit: false,
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
        }
    }

    /// Request application exit
    pub(crate) fn exit(&mut self) {
        self.exit = true;
    }

    /// Get reference to active functions data based on selected tab
    pub(crate) fn active_functions(&self) -> &FunctionsJson {
        match self.selected_tab {
            SelectedTab::Timing => &self.timing_functions,
            SelectedTab::Memory => &self.memory_functions,
            _ => unreachable!("active_functions() called on non-functions tab"),
        }
    }

    /// Get mutable reference to active table state based on selected tab
    pub(crate) fn active_table_state_mut(&mut self) -> &mut TableState {
        match self.selected_tab {
            SelectedTab::Timing => &mut self.timing_table_state,
            SelectedTab::Memory => &mut self.memory_table_state,
            SelectedTab::Channels => &mut self.channels_table_state,
            SelectedTab::Streams => &mut self.streams_table_state,
        }
    }

    /// Main TUI run loop
    ///
    /// This runs the event loop, handling:
    /// - Periodic data refresh (unless paused)
    /// - Rendering the UI
    /// - Processing keyboard input
    pub(crate) fn run(
        &mut self,
        terminal: &mut ratatui::Terminal<ratatui::backend::CrosstermBackend<std::io::Stdout>>,
        refresh_interval_ms: u64,
    ) -> std::io::Result<()> {
        use crossterm::event::{self, Event, KeyEventKind};

        let refresh_interval = Duration::from_millis(refresh_interval_ms);

        self.refresh_data();

        while !self.exit {
            if !self.paused && self.last_refresh.elapsed() >= refresh_interval {
                self.refresh_data();
            }

            terminal.draw(|frame| super::views::render_ui(frame, self))?;

            if event::poll(super::constants::event_poll_interval())? {
                if let Event::Key(key_event) = event::read()? {
                    if key_event.kind == KeyEventKind::Press {
                        self.handle_key_event(key_event.code);
                    }
                }
            }
        }

        Ok(())
    }
}

// Note: Most App methods are implemented in submodules for better organization:
// - app/state.rs: Navigation and selection methods (next_function, toggle_logs, etc.)
// - app/data.rs: Data fetching and update methods (update_metrics, refresh_data, etc.)
// - app/keys.rs: Keyboard input handling (handle_key_event)
