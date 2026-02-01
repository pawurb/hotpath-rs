#[cfg(target_os = "linux")]
use quanta::Instant;

#[cfg(not(target_os = "linux"))]
use std::time::Instant;

use prettytable::{Cell, Row, Table};
use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;

use crate::channels::{compare_channel_entries, resolve_label, ChannelEntry, CHANNELS_STATE};
use crate::json::{JsonChannelEntry, JsonChannelsList};
use crate::output::{format_bytes, resolve_output_path};
use crate::Format;

/// Builder for creating a ChannelsGuard with custom configuration.
///
/// # Examples
///
/// ```no_run
/// use channels_console::{ChannelsGuardBuilder, Format};
///
/// let _guard = ChannelsGuardBuilder::new()
///     .format(Format::JsonPretty)
///     .build();
/// // Statistics will be printed as pretty JSON when _guard is dropped
/// ```
#[must_use = "builder is discarded without creating a guard"]
pub struct ChannelsGuardBuilder {
    format: Format,
    output_path: Option<PathBuf>,
}

impl ChannelsGuardBuilder {
    /// Create a new channels guard builder.
    pub fn new() -> Self {
        Self {
            format: Format::default(),
            output_path: None,
        }
    }

    /// Set the output format for statistics.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use channels_console::{ChannelsGuardBuilder, Format};
    ///
    /// let _guard = ChannelsGuardBuilder::new()
    ///     .format(Format::Json)
    ///     .build();
    /// ```
    pub fn format(mut self, format: Format) -> Self {
        self.format = format;
        self
    }

    /// Sets the output file path for the channels statistics report.
    pub fn output_path(mut self, path: impl AsRef<std::path::Path>) -> Self {
        self.output_path = Some(resolve_output_path(path));
        self
    }

    /// Build and return the ChannelsGuard.
    /// Statistics will be printed when the guard is dropped.
    pub fn build(self) -> ChannelsGuard {
        ChannelsGuard {
            start_time: Instant::now(),
            format: self.format,
            output_path: self.output_path,
        }
    }
}

impl Default for ChannelsGuardBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Guard for channel statistics collection.
/// When dropped, prints a summary of all instrumented channels and their statistics.
///
/// Use `ChannelsGuardBuilder` to create a guard with custom configuration.
///
/// # Examples
///
/// ```no_run
/// use channels_console::ChannelsGuard;
///
/// let _guard = ChannelsGuard::new();
/// // Your code with instrumented channels here
/// // Statistics will be printed when _guard is dropped
/// ```
#[must_use = "guard is dropped immediately without printing statistics"]
pub struct ChannelsGuard {
    start_time: Instant,
    format: Format,
    output_path: Option<PathBuf>,
}

impl ChannelsGuard {
    /// Create a new channels guard with default settings (table format).
    /// Statistics will be printed when this guard is dropped.
    ///
    /// For custom configuration, use `ChannelsGuardBuilder::new()` instead.
    pub fn new() -> Self {
        Self {
            start_time: Instant::now(),
            format: Format::default(),
            output_path: None,
        }
    }

    /// Set the output format for statistics.
    /// This is a convenience method for backward compatibility.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use channels_console::{ChannelsGuard, Format};
    ///
    /// let _guard = ChannelsGuard::new().format(Format::Json);
    /// ```
    pub fn format(mut self, format: Format) -> Self {
        self.format = format;
        self
    }

    /// Sets the output file path for the channels statistics report.
    pub fn output_path(mut self, path: impl AsRef<std::path::Path>) -> Self {
        self.output_path = Some(resolve_output_path(path));
        self
    }
}

impl Default for ChannelsGuard {
    fn default() -> Self {
        Self::new()
    }
}

fn get_sorted_channels(stats: HashMap<u64, ChannelEntry>) -> Vec<ChannelEntry> {
    let mut channels: Vec<ChannelEntry> = stats.into_values().collect();
    channels.sort_by(compare_channel_entries);
    channels
}

impl Drop for ChannelsGuard {
    fn drop(&mut self) {
        let elapsed = self.start_time.elapsed();

        let channels = CHANNELS_STATE
            .get()
            .and_then(|state| {
                if let Ok(mut guard) = state.shutdown_tx.lock() {
                    if let Some(tx) = guard.take() {
                        let _ = tx.send(());
                    }
                }
                state
                    .completion_rx
                    .lock()
                    .ok()
                    .and_then(|mut guard| guard.take())
                    .and_then(|rx| rx.recv().ok())
            })
            .map(get_sorted_channels)
            .unwrap_or_default();

        let output = crate::output::OutputDestination::from_path(self.output_path.take());
        let mut writer: Box<dyn Write> = match output.writer() {
            Ok(w) => w,
            Err(e) => {
                eprintln!("Failed to create output writer: {}", e);
                return;
            }
        };

        if channels.is_empty() {
            let _ = writeln!(writer, "\nNo instrumented channels found.");
            return;
        }

        let format = if std::env::var("HOTPATH_OUTPUT_FORMAT").is_ok() {
            Format::from_env()
        } else {
            self.format
        };

        match format {
            Format::Table => {
                let _ = writeln!(
                    writer,
                    "\n=== Channel Statistics (runtime: {:.2}s) ===",
                    elapsed.as_secs_f64()
                );

                let mut table = Table::new();

                table.add_row(Row::new(vec![
                    Cell::new("Channel"),
                    Cell::new("Type"),
                    Cell::new("State"),
                    Cell::new("Sent"),
                    Cell::new("Received"),
                    Cell::new("Queued"),
                    Cell::new("Mem"),
                ]));

                for channel_stats in &channels {
                    let label = resolve_label(
                        channel_stats.source,
                        channel_stats.label.as_deref(),
                        Some(channel_stats.iter),
                    );
                    table.add_row(Row::new(vec![
                        Cell::new(&label),
                        Cell::new(&channel_stats.channel_type.to_string()),
                        Cell::new(channel_stats.state.as_str()),
                        Cell::new(&channel_stats.sent_count.to_string()),
                        Cell::new(&channel_stats.received_count.to_string()),
                        Cell::new(&channel_stats.queued().to_string()),
                        Cell::new(&format_bytes(channel_stats.queued_bytes())),
                    ]));
                }

                let _ = writeln!(writer, "\nChannels:");
                let _ = table.print(&mut writer);
            }
            Format::Json => {
                let channels_json = JsonChannelsList {
                    current_elapsed_ns: elapsed.as_nanos() as u64,
                    channels: channels.iter().map(JsonChannelEntry::from).collect(),
                };
                match serde_json::to_string(&channels_json) {
                    Ok(json) => {
                        let _ = writeln!(writer, "{}", json);
                    }
                    Err(e) => eprintln!("Failed to serialize statistics to JSON: {}", e),
                }
            }
            Format::JsonPretty => {
                let channels_json = JsonChannelsList {
                    current_elapsed_ns: elapsed.as_nanos() as u64,
                    channels: channels.iter().map(JsonChannelEntry::from).collect(),
                };
                match serde_json::to_string_pretty(&channels_json) {
                    Ok(json) => {
                        let _ = writeln!(writer, "{}", json);
                    }
                    Err(e) => eprintln!("Failed to serialize statistics to pretty JSON: {}", e),
                }
            }
        }
    }
}
