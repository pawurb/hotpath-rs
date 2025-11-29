#[cfg(target_os = "linux")]
use quanta::Instant;

#[cfg(not(target_os = "linux"))]
use std::time::Instant;

use prettytable::{Cell, Row, Table};

use crate::channels::{resolve_label, Format};
use crate::streams::{get_sorted_stream_stats, SerializableStreamStats, StreamsJson};

/// Builder for creating a StreamsGuard with custom configuration.
///
/// # Examples
///
/// ```no_run
/// use streams_console::{StreamsGuardBuilder, Format};
///
/// let _guard = StreamsGuardBuilder::new()
///     .format(Format::JsonPretty)
///     .build();
/// // Statistics will be printed as pretty JSON when _guard is dropped
/// ```
pub struct StreamsGuardBuilder {
    format: Format,
}

impl StreamsGuardBuilder {
    /// Create a new streams guard builder.
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
    /// use streams_console::{StreamsGuardBuilder, Format};
    ///
    /// let _guard = StreamsGuardBuilder::new()
    ///     .format(Format::Json)
    ///     .build();
    /// ```
    pub fn format(mut self, format: Format) -> Self {
        self.format = format;
        self
    }

    /// Build and return the StreamsGuard.
    /// Statistics will be printed when the guard is dropped.
    pub fn build(self) -> StreamsGuard {
        StreamsGuard {
            start_time: Instant::now(),
            format: self.format,
        }
    }
}

impl Default for StreamsGuardBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Guard for stream statistics collection.
/// When dropped, prints a summary of all instrumented streams and their statistics.
///
/// Use `StreamsGuardBuilder` to create a guard with custom configuration.
///
/// # Examples
///
/// ```no_run
/// use streams_console::StreamsGuard;
///
/// let _guard = StreamsGuard::new();
/// // Your code with instrumented streams here
/// // Statistics will be printed when _guard is dropped
/// ```
pub struct StreamsGuard {
    start_time: Instant,
    format: Format,
}

impl StreamsGuard {
    /// Create a new streams guard with default settings (table format).
    /// Statistics will be printed when this guard is dropped.
    ///
    /// For custom configuration, use `StreamsGuardBuilder::new()` instead.
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
    /// use streams_console::{StreamsGuard, Format};
    ///
    /// let _guard = StreamsGuard::new().format(Format::Json);
    /// ```
    pub fn format(mut self, format: Format) -> Self {
        self.format = format;
        self
    }
}

impl Default for StreamsGuard {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for StreamsGuard {
    fn drop(&mut self) {
        let elapsed = self.start_time.elapsed();
        let streams = get_sorted_stream_stats();

        if streams.is_empty() {
            println!("\nNo instrumented streams found.");
            return;
        }

        match self.format {
            Format::Table => {
                println!(
                    "\n=== Stream Statistics (runtime: {:.2}s) ===",
                    elapsed.as_secs_f64()
                );

                let mut table = Table::new();

                table.add_row(Row::new(vec![
                    Cell::new("Stream"),
                    Cell::new("State"),
                    Cell::new("Yielded"),
                ]));

                for stream_stats in streams {
                    let label = resolve_label(
                        stream_stats.source,
                        stream_stats.label.as_deref(),
                        Some(stream_stats.iter),
                    );
                    table.add_row(Row::new(vec![
                        Cell::new(&label),
                        Cell::new(stream_stats.state.as_str()),
                        Cell::new(&stream_stats.items_yielded.to_string()),
                    ]));
                }

                println!("\nStreams:");
                table.printstd();
            }
            Format::Json => {
                let streams_json = StreamsJson {
                    current_elapsed_ns: elapsed.as_nanos() as u64,
                    streams: streams.iter().map(SerializableStreamStats::from).collect(),
                };
                match serde_json::to_string(&streams_json) {
                    Ok(json) => println!("{}", json),
                    Err(e) => eprintln!("Failed to serialize statistics to JSON: {}", e),
                }
            }
            Format::JsonPretty => {
                let streams_json = StreamsJson {
                    current_elapsed_ns: elapsed.as_nanos() as u64,
                    streams: streams.iter().map(SerializableStreamStats::from).collect(),
                };
                match serde_json::to_string_pretty(&streams_json) {
                    Ok(json) => println!("{}", json),
                    Err(e) => eprintln!("Failed to serialize statistics to pretty JSON: {}", e),
                }
            }
        }
    }
}
