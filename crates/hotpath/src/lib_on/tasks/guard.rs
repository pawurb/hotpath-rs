#[cfg(target_os = "linux")]
use quanta::Instant;

#[cfg(not(target_os = "linux"))]
use std::time::Instant;

use prettytable::{Cell, Row, Table};

use crate::channels::{resolve_label, Format};
use crate::tasks::{get_sorted_task_stats, init_tasks_state, SerializableTaskStats, TasksJson};

/// Builder for creating a FuturesGuard with custom configuration.
///
/// # Examples
///
/// ```no_run
/// use hotpath::tasks::{FuturesGuardBuilder, Format};
///
/// let _guard = FuturesGuardBuilder::new()
///     .format(Format::JsonPretty)
///     .build();
/// // Statistics will be printed as pretty JSON when _guard is dropped
/// ```
pub struct FuturesGuardBuilder {
    format: Format,
}

impl FuturesGuardBuilder {
    /// Create a new futures guard builder.
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
    /// use hotpath::tasks::{FuturesGuardBuilder, Format};
    ///
    /// let _guard = FuturesGuardBuilder::new()
    ///     .format(Format::Json)
    ///     .build();
    /// ```
    pub fn format(mut self, format: Format) -> Self {
        self.format = format;
        self
    }

    /// Build and return the FuturesGuard.
    /// Statistics will be printed when the guard is dropped.
    pub fn build(self) -> FuturesGuard {
        init_tasks_state();
        FuturesGuard {
            start_time: Instant::now(),
            format: self.format,
        }
    }
}

impl Default for FuturesGuardBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Guard for future statistics collection.
/// When dropped, prints a summary of all instrumented futures and their statistics.
///
/// Use `FuturesGuardBuilder` to create a guard with custom configuration.
///
/// # Examples
///
/// ```no_run
/// use hotpath::tasks::FuturesGuard;
///
/// let _guard = FuturesGuard::new();
/// // Your code with instrumented futures here
/// // Statistics will be printed when _guard is dropped
/// ```
pub struct FuturesGuard {
    start_time: Instant,
    format: Format,
}

impl FuturesGuard {
    /// Create a new futures guard with default settings (table format).
    /// Statistics will be printed when this guard is dropped.
    ///
    /// For custom configuration, use `FuturesGuardBuilder::new()` instead.
    pub fn new() -> Self {
        init_tasks_state();
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
    /// use hotpath::tasks::{FuturesGuard, Format};
    ///
    /// let _guard = FuturesGuard::new().format(Format::Json);
    /// ```
    pub fn format(mut self, format: Format) -> Self {
        self.format = format;
        self
    }
}

impl Default for FuturesGuard {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for FuturesGuard {
    fn drop(&mut self) {
        let elapsed = self.start_time.elapsed();
        let futures = get_sorted_task_stats();

        if futures.is_empty() {
            println!("\nNo instrumented futures found.");
            return;
        }

        match self.format {
            Format::Table => {
                println!(
                    "\n=== Future Statistics (runtime: {:.2}s) ===",
                    elapsed.as_secs_f64()
                );

                let mut table = Table::new();

                table.add_row(Row::new(vec![
                    Cell::new("Future"),
                    Cell::new("State"),
                    Cell::new("Polls"),
                    Cell::new("Result"),
                ]));

                for future_stats in futures {
                    let label = resolve_label(
                        future_stats.source,
                        future_stats.label.as_deref(),
                        future_stats.iter,
                    );
                    let result = match future_stats.get_result() {
                        Some(s) => shorten_result(s),
                        None => {
                            // If ready but no result, it means log=true wasn't used
                            if future_stats.state == crate::tasks::TaskState::Ready {
                                "N/A".to_string()
                            } else {
                                "-".to_string()
                            }
                        }
                    };

                    table.add_row(Row::new(vec![
                        Cell::new(&label),
                        Cell::new(future_stats.state.as_str()),
                        Cell::new(&future_stats.poll_count.to_string()),
                        Cell::new(&result),
                    ]));
                }

                println!("\nFutures:");
                table.printstd();
            }
            Format::Json => {
                let futures_json = TasksJson {
                    current_elapsed_ns: elapsed.as_nanos() as u64,
                    tasks: futures.iter().map(SerializableTaskStats::from).collect(),
                };
                match serde_json::to_string(&futures_json) {
                    Ok(json) => println!("{}", json),
                    Err(e) => eprintln!("Failed to serialize statistics to JSON: {}", e),
                }
            }
            Format::JsonPretty => {
                let futures_json = TasksJson {
                    current_elapsed_ns: elapsed.as_nanos() as u64,
                    tasks: futures.iter().map(SerializableTaskStats::from).collect(),
                };
                match serde_json::to_string_pretty(&futures_json) {
                    Ok(json) => println!("{}", json),
                    Err(e) => eprintln!("Failed to serialize statistics to pretty JSON: {}", e),
                }
            }
        }
    }
}

/// Shorten a result string for display.
fn shorten_result(result: &str) -> String {
    if result.len() > 50 {
        format!("{}...", &result[..47])
    } else {
        result.to_string()
    }
}
