//! Data management - fetching, updating, and transforming metrics/channels/samples

use super::{App, CachedLogs, SelectedTab};
use hotpath::{MetricsJson, SamplesJson};
use std::collections::HashMap;
use std::time::Instant;

impl App {
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
        // Capture the currently selected channel ID (not index!)
        let selected_channel_id = self
            .table_state
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
                self.table_state.select(Some(new_idx));
            } else {
                // Channel no longer exists, select the last one if available
                if !self.channels.channels.is_empty() {
                    self.table_state
                        .select(Some(self.channels.channels.len() - 1));
                }
            }
        } else if let Some(selected) = self.table_state.selected() {
            if selected >= self.channels.channels.len() && !self.channels.channels.is_empty() {
                self.table_state
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

        if let Some(selected) = self.table_state.selected() {
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
                        if let Some(selected) = self.logs_table_state.selected() {
                            if selected >= log_count && log_count > 0 {
                                self.logs_table_state.select(Some(log_count - 1));
                            }
                        }
                    }
                }
            }
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
                match super::super::http::fetch_samples(&self.agent, port, function_name) {
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

    pub(crate) fn refresh_data(&mut self) {
        match self.selected_tab {
            SelectedTab::Metrics => {
                match super::super::http::fetch_metrics(&self.agent, self.metrics_port) {
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
                match super::super::http::fetch_channels(&self.agent, self.metrics_port) {
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
}
