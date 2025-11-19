use crossterm::event::KeyCode;
use hotpath::{MetricsJson, SamplesJson};
use ratatui::widgets::TableState;
use std::time::{Duration, Instant};

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

pub(crate) struct App {
    pub(crate) metrics: MetricsJson,
    pub(crate) channels: hotpath::channels::ChannelsJson,
    pub(crate) table_state: TableState,
    pub(crate) selected_tab: SelectedTab,
    pub(crate) paused: bool,
    pub(crate) last_refresh: Instant,
    pub(crate) last_successful_fetch: Option<Instant>,
    pub(crate) error_message: Option<String>,
    pub(crate) show_samples: bool,
    pub(crate) current_samples: Option<SamplesJson>,
    pub(crate) pinned_function: Option<String>,
    pub(crate) agent: ureq::Agent,
    pub(crate) metrics_port: u16,
    exit: bool,
}

impl App {
    pub(crate) fn new(metrics_port: u16) -> Self {
        let config = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_millis(2000)))
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
        }
    }

    pub(crate) fn next_function(&mut self) {
        let function_count = self.metrics.data.0.len();
        if function_count == 0 {
            return;
        }

        let i = match self.table_state.selected() {
            Some(i) => (i + 1).min(function_count - 1), // Bounded, stop at last
            None => 0,
        };
        self.table_state.select(Some(i));
    }

    pub(crate) fn previous_function(&mut self) {
        let function_count = self.metrics.data.0.len();
        if function_count == 0 {
            return;
        }

        let i = match self.table_state.selected() {
            Some(i) => i.saturating_sub(1), // Bounded, stop at 0
            None => 0,
        };
        self.table_state.select(Some(i));
    }

    pub(crate) fn toggle_pause(&mut self) {
        self.paused = !self.paused;
    }

    pub(crate) fn switch_to_tab(&mut self, tab: SelectedTab) {
        self.selected_tab = tab;
    }

    pub(crate) fn update_metrics(&mut self, metrics: MetricsJson) {
        // Capture the currently selected function name (not index!)
        let selected_function_name = self.selected_function_name();

        self.metrics = metrics;
        self.last_successful_fetch = Some(Instant::now());
        self.error_message = None;

        let sorted_entries = self.get_sorted_entries();

        if let Some(function_name) = selected_function_name {
            // Find the new index of the previously selected function in sorted order
            if let Some(new_idx) = sorted_entries
                .iter()
                .position(|(name, _)| name == &function_name)
            {
                self.table_state.select(Some(new_idx));
            } else {
                // Function no longer exists, select the last one
                if !sorted_entries.is_empty() {
                    self.table_state.select(Some(sorted_entries.len() - 1));
                }
            }
        } else if let Some(selected) = self.table_state.selected() {
            // Bound check: if current selection is now out of bounds
            if selected >= sorted_entries.len() && !sorted_entries.is_empty() {
                self.table_state.select(Some(sorted_entries.len() - 1));
            }
        } else if !sorted_entries.is_empty() {
            // No selection yet, select first item
            self.table_state.select(Some(0));
        }
    }

    pub(crate) fn set_error(&mut self, error: String) {
        self.error_message = Some(error);
    }

    pub(crate) fn update_channels(&mut self, channels: hotpath::channels::ChannelsJson) {
        self.channels = channels;
        self.last_successful_fetch = Some(Instant::now());
        self.error_message = None;
    }

    pub(crate) fn toggle_samples(&mut self) {
        self.show_samples = !self.show_samples;
        if self.show_samples {
            // Pin the currently selected function when opening samples panel
            self.pinned_function = self.selected_function_name();
        } else {
            // Clear pinned function when closing samples panel
            self.pinned_function = None;
        }
    }

    /// Get sorted entries (sorted by percentage, highest first)
    pub(crate) fn get_sorted_entries(&self) -> Vec<(String, Vec<hotpath::MetricType>)> {
        use hotpath::MetricType;

        let mut entries: Vec<(String, Vec<MetricType>)> = self
            .metrics
            .data
            .0
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();

        entries.sort_by(|(_, metrics_a), (_, metrics_b)| {
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

            percent_b.cmp(&percent_a)
        });

        entries
    }

    pub(crate) fn selected_function_name(&self) -> Option<String> {
        let sorted_entries = self.get_sorted_entries();
        self.table_state
            .selected()
            .and_then(|idx| sorted_entries.get(idx).map(|(name, _)| name.clone()))
    }

    pub(crate) fn update_samples(&mut self, samples: SamplesJson) {
        self.current_samples = Some(samples);
    }

    pub(crate) fn clear_samples(&mut self) {
        self.current_samples = None;
    }

    pub(crate) fn update_pinned_function(&mut self) {
        if self.show_samples {
            self.pinned_function = self.selected_function_name();
        }
    }

    pub(crate) fn samples_function_name(&self) -> Option<&str> {
        self.pinned_function.as_deref()
    }

    /// Fetch samples for pinned function if panel is open
    pub(crate) fn fetch_samples_if_open(&mut self, port: u16) {
        if self.show_samples {
            if let Some(function_name) = self.samples_function_name() {
                match super::http::fetch_samples(&self.agent, port, function_name) {
                    Ok(samples) => self.update_samples(samples),
                    Err(_) => self.clear_samples(),
                }
            }
        }
    }

    /// Update pinned function and fetch samples if panel is open
    pub(crate) fn update_and_fetch_samples(&mut self, port: u16) {
        self.update_pinned_function();
        self.fetch_samples_if_open(port);
    }

    pub(crate) fn exit(&mut self) {
        self.exit = true;
    }

    fn refresh_data(&mut self) {
        match self.selected_tab {
            SelectedTab::Metrics => {
                match super::http::fetch_metrics(&self.agent, self.metrics_port) {
                    Ok(metrics) => {
                        self.update_metrics(metrics);
                    }
                    Err(e) => {
                        self.set_error(format!("{}", e));
                    }
                }
                self.fetch_samples_if_open(self.metrics_port);
            }
            SelectedTab::Channels => {
                match super::http::fetch_channels(&self.agent, self.metrics_port) {
                    Ok(channels) => {
                        self.update_channels(channels);
                    }
                    Err(e) => {
                        self.set_error(format!("{}", e));
                    }
                }
            }
        }
        self.last_refresh = Instant::now();
    }

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

            if event::poll(Duration::from_millis(100))? {
                if let Event::Key(key_event) = event::read()? {
                    if key_event.kind == KeyEventKind::Press {
                        self.handle_key_event(key_event.code);
                    }
                }
            }
        }

        Ok(())
    }

    fn handle_key_event(&mut self, key_code: KeyCode) {
        match key_code {
            KeyCode::Char('q') | KeyCode::Char('Q') => self.exit(),
            KeyCode::Char('p') | KeyCode::Char('P') => self.toggle_pause(),
            KeyCode::Char('1') => {
                self.switch_to_tab(SelectedTab::Metrics);
                self.refresh_data();
            }
            KeyCode::Char('2') => {
                self.switch_to_tab(SelectedTab::Channels);
                self.refresh_data();
            }
            KeyCode::Char('o') | KeyCode::Char('O') => {
                self.toggle_samples();
                self.fetch_samples_if_open(self.metrics_port);
            }
            KeyCode::Char('j') | KeyCode::Down => {
                self.next_function();
                self.update_and_fetch_samples(self.metrics_port);
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.previous_function();
                self.update_and_fetch_samples(self.metrics_port);
            }
            _ => {}
        }
    }
}
