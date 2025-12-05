#[cfg(target_os = "linux")]
use quanta::Instant;

#[cfg(not(target_os = "linux"))]
use std::time::Instant;

use prettytable::{Cell, Row, Table};

use crate::channels::{get_sorted_channel_stats, resolve_label};
use crate::output::format_bytes;
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
pub struct ChannelsGuardBuilder {
    format: Format,
}

impl ChannelsGuardBuilder {
    /// Create a new channels guard builder.
    pub fn new() -> Self {
        Self {
            format: Format::default(),
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

    /// Build and return the ChannelsGuard.
    /// Statistics will be printed when the guard is dropped.
    pub fn build(self) -> ChannelsGuard {
        ChannelsGuard {
            start_time: Instant::now(),
            format: self.format,
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
pub struct ChannelsGuard {
    start_time: Instant,
    format: Format,
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
}

impl Default for ChannelsGuard {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for ChannelsGuard {
    fn drop(&mut self) {
        let elapsed = self.start_time.elapsed();
        let channels = get_sorted_channel_stats();

        if channels.is_empty() {
            println!("\nNo instrumented channels found.");
            return;
        }

        match self.format {
            Format::Table => {
                println!(
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

                for channel_stats in channels {
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

                println!("\nChannels:");
                table.printstd();
            }
            Format::Json => {
                let channels_json = crate::channels::ChannelsJson {
                    current_elapsed_ns: elapsed.as_nanos() as u64,
                    channels: channels
                        .iter()
                        .map(crate::channels::SerializableChannelStats::from)
                        .collect(),
                };
                match serde_json::to_string(&channels_json) {
                    Ok(json) => println!("{}", json),
                    Err(e) => eprintln!("Failed to serialize statistics to JSON: {}", e),
                }
            }
            Format::JsonPretty => {
                let channels_json = crate::channels::ChannelsJson {
                    current_elapsed_ns: elapsed.as_nanos() as u64,
                    channels: channels
                        .iter()
                        .map(crate::channels::SerializableChannelStats::from)
                        .collect(),
                };
                match serde_json::to_string_pretty(&channels_json) {
                    Ok(json) => println!("{}", json),
                    Err(e) => eprintln!("Failed to serialize statistics to pretty JSON: {}", e),
                }
            }
        }
    }
}
