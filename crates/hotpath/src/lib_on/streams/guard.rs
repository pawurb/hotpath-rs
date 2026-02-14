#[cfg(target_os = "linux")]
use quanta::Instant;

#[cfg(not(target_os = "linux"))]
use std::time::Instant;

use prettytable::{Cell, Row, Table};
use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;
use std::thread;

use crate::channels::resolve_label;
use crate::json::{JsonStreamEntry, JsonStreamsList};
use crate::output::resolve_output_path;
use crate::streams::{compare_stream_stats, StreamStats, STREAMS_STATE};
use crate::Format;

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
#[must_use = "builder is discarded without creating a guard"]
pub struct StreamsGuardBuilder {
    format: Format,
    output_path: Option<PathBuf>,
}

#[cfg_attr(feature = "hotpath-meta", hotpath_meta::measure_all)]
impl StreamsGuardBuilder {
    /// Create a new streams guard builder.
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

    /// Sets the output file path for the streams statistics report.
    pub fn output_path(mut self, path: impl AsRef<std::path::Path>) -> Self {
        self.output_path = Some(resolve_output_path(path));
        self
    }

    /// Build and return the StreamsGuard.
    /// Statistics will be printed when the guard is dropped.
    pub fn build(self) -> StreamsGuard {
        StreamsGuard {
            start_time: Instant::now(),
            format: self.format,
            output_path: self.output_path,
        }
    }

    /// Builds the streams guard and automatically drops it after the specified duration and exits the program.
    ///
    /// # Arguments
    ///
    /// * `duration` - The duration to wait before dropping the guard and generating the report
    pub fn build_with_timeout(self, duration: std::time::Duration) {
        let guard = self.build();
        thread::spawn(move || {
            thread::sleep(duration);
            drop(guard);
            std::process::exit(0);
        });
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
#[must_use = "guard is dropped immediately without printing statistics"]
pub struct StreamsGuard {
    start_time: Instant,
    format: Format,
    output_path: Option<PathBuf>,
}

#[cfg_attr(feature = "hotpath-meta", hotpath_meta::measure_all)]
impl StreamsGuard {
    /// Create a new streams guard with default settings (table format).
    /// Statistics will be printed when this guard is dropped.
    ///
    /// For custom configuration, use `StreamsGuardBuilder::new()` instead.
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
    /// use streams_console::{StreamsGuard, Format};
    ///
    /// let _guard = StreamsGuard::new().format(Format::Json);
    /// ```
    pub fn format(mut self, format: Format) -> Self {
        self.format = format;
        self
    }

    /// Sets the output file path for the streams statistics report.
    pub fn output_path(mut self, path: impl AsRef<std::path::Path>) -> Self {
        self.output_path = Some(resolve_output_path(path));
        self
    }
}

impl Default for StreamsGuard {
    fn default() -> Self {
        Self::new()
    }
}

fn get_sorted_streams(stats: HashMap<u64, StreamStats>) -> Vec<StreamStats> {
    let mut streams: Vec<StreamStats> = stats.into_values().collect();
    streams.sort_by(compare_stream_stats);
    streams
}

impl Drop for StreamsGuard {
    fn drop(&mut self) {
        let elapsed = self.start_time.elapsed();

        let streams = STREAMS_STATE
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
                    .and_then(|rx| rx.recv().ok());
                state.stats_map.read().ok().map(|stats| stats.clone())
            })
            .map(get_sorted_streams)
            .unwrap_or_default();

        let output = crate::output::OutputDestination::from_path(self.output_path.take());
        let mut writer: Box<dyn Write> = match output.writer() {
            Ok(w) => w,
            Err(e) => {
                eprintln!("Failed to create output writer: {}", e);
                return;
            }
        };

        if streams.is_empty() {
            let _ = writeln!(writer, "\nNo instrumented streams found.");
            return;
        }

        let format = if std::env::var("HOTPATH_OUTPUT_FORMAT").is_ok() {
            Format::from_env()
        } else {
            self.format
        };

        match format {
            Format::None => (),
            Format::Table => {
                let _ = writeln!(
                    writer,
                    "\n=== Stream Statistics (runtime: {:.2}s) ===",
                    elapsed.as_secs_f64()
                );

                let mut table = Table::new();

                table.add_row(Row::new(vec![
                    Cell::new("Stream"),
                    Cell::new("State"),
                    Cell::new("Yielded"),
                ]));

                for stream_stats in &streams {
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

                let _ = writeln!(writer, "\nStreams:");
                let _ = table.print(&mut writer);
            }
            Format::Json => {
                let streams_json = JsonStreamsList {
                    current_elapsed_ns: elapsed.as_nanos() as u64,
                    streams: streams.iter().map(JsonStreamEntry::from).collect(),
                };
                match serde_json::to_string(&streams_json) {
                    Ok(json) => {
                        let _ = writeln!(writer, "{}", json);
                    }
                    Err(e) => eprintln!("Failed to serialize statistics to JSON: {}", e),
                }
            }
            Format::JsonPretty => {
                let streams_json = JsonStreamsList {
                    current_elapsed_ns: elapsed.as_nanos() as u64,
                    streams: streams.iter().map(JsonStreamEntry::from).collect(),
                };
                match serde_json::to_string_pretty(&streams_json) {
                    Ok(json) => {
                        let _ = writeln!(writer, "{}", json);
                    }
                    Err(e) => eprintln!("Failed to serialize statistics to pretty JSON: {}", e),
                }
            }
        }
    }
}
