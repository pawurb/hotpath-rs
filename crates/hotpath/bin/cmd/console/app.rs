//! TUI application state and main run loop
//!
//! The App struct manages all TUI state including metrics, channels, UI state,
//! and user interactions. Implementation is split across modules:
//! - `state`: Navigation and selection logic
//! - `data`: Data fetching and updates
//! - `keys`: Keyboard input handling

use hotpath::channels::{ChannelLogs, LogEntry};
use hotpath::{MetricsJson, SamplesJson};
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
    Metrics,
    Channels,
}

impl SelectedTab {
    pub(crate) fn title(&self) -> &'static str {
        match self {
            SelectedTab::Metrics => "1 Metrics",
            SelectedTab::Channels => "2 Channels",
        }
    }
}

/// Represents which UI component has focus in the Channels tab
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Focus {
    Channels,
    Logs,
    Inspect,
}

/// Cached logs with a lookup map for received entries
pub(crate) struct CachedLogs {
    pub(crate) logs: ChannelLogs,
    pub(crate) received_map: HashMap<u64, LogEntry>,
}

/// Main TUI application state
///
/// This struct manages all application state including:
/// - Metrics data and channels data from the profiled application
/// - UI state (selected tab, table selections, focus)
/// - HTTP client for fetching data from the metrics server
/// - Error handling and connection status
///
/// The implementation is split across multiple modules to improve maintainability.
pub(crate) struct App {
    // Data from profiled application
    /// Current metrics data (functions, timings, allocations)
    pub(crate) metrics: MetricsJson,
    /// Current channels data (message passing statistics)
    pub(crate) channels: hotpath::channels::ChannelsJson,

    // UI state - navigation and selection
    /// Selection state for main table (functions or channels)
    pub(crate) table_state: TableState,
    /// Currently selected tab (Metrics or Channels)
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

    // Samples panel (Metrics tab)
    /// Whether samples panel is visible
    pub(crate) show_samples: bool,
    /// Current samples data for selected function
    pub(crate) current_samples: Option<SamplesJson>,
    /// Function pinned for samples display
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
    pub(crate) logs_table_state: TableState,
    /// Which component has focus in Channels tab
    pub(crate) focus: Focus,
    /// Whether logs panel is visible
    pub(crate) show_logs: bool,
    /// Cached logs data for selected channel
    pub(crate) logs: Option<CachedLogs>,
    /// Log entry being inspected in popup
    pub(crate) inspected_log: Option<LogEntry>,
}

impl App {
    /// Create a new App instance
    pub(crate) fn new(metrics_port: u16) -> Self {
        let config = ureq::Agent::config_builder()
            .timeout_global(Some(super::constants::http_timeout()))
            .build();
        let agent: ureq::Agent = config.into();

        Self {
            metrics: MetricsJson {
                hotpath_profiling_mode: hotpath::ProfilingMode::Timing,
                total_elapsed: 0,
                description: "Waiting for data...".to_string(),
                caller_name: "unknown".to_string(),
                percentiles: vec![95],
                data: hotpath::MetricsDataJson(std::collections::HashMap::new()),
            },
            channels: hotpath::channels::ChannelsJson {
                current_elapsed_ns: 0,
                channels: vec![],
            },
            table_state: TableState::default().with_selected(0),
            selected_tab: SelectedTab::default(),
            paused: false,
            last_refresh: Instant::now(),
            last_successful_fetch: None,
            error_message: None,
            show_samples: false,
            current_samples: None,
            pinned_function: None,
            agent,
            metrics_port,
            exit: false,
            logs_table_state: TableState::default(),
            focus: Focus::Channels,
            show_logs: false,
            logs: None,
            inspected_log: None,
        }
    }

    /// Request application exit
    pub(crate) fn exit(&mut self) {
        self.exit = true;
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
