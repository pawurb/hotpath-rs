#[cfg(target_os = "linux")]
use quanta::Instant;

#[cfg(not(target_os = "linux"))]
use std::time::Instant;

use prettytable::{Cell, Row, Table};

use crate::formatted::FormattedFuturesJson;
use crate::futures::{get_futures_json, init_futures_state};
use crate::Format;

/// Builder for creating a FuturesGuard with custom configuration.
///
/// # Examples
///
/// ```no_run
/// use hotpath::futures::{FuturesGuardBuilder, Format};
///
/// let _guard = FuturesGuardBuilder::new()
///     .format(Format::JsonPretty)
///     .build();
/// // Statistics will be printed as pretty JSON when _guard is dropped
/// ```
#[must_use = "builder is discarded without creating a guard"]
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
    /// use hotpath::futures::{FuturesGuardBuilder, Format};
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
        init_futures_state();
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
/// use hotpath::futures::FuturesGuard;
///
/// let _guard = FuturesGuard::new();
/// // Your code with instrumented futures here
/// // Statistics will be printed when _guard is dropped
/// ```
#[must_use = "guard is dropped immediately without printing statistics"]
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
        init_futures_state();
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
    /// use hotpath::futures::{FuturesGuard, Format};
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
        let futures_json = get_futures_json();

        if futures_json.futures.is_empty() {
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
                    Cell::new("Calls"),
                    Cell::new("Polls"),
                ]));

                for future_stats in &futures_json.futures {
                    table.add_row(Row::new(vec![
                        Cell::new(&future_stats.label),
                        Cell::new(&future_stats.call_count.to_string()),
                        Cell::new(&future_stats.total_polls.to_string()),
                    ]));
                }

                println!("\nFutures:");
                table.printstd();
            }
            Format::Json => {
                let json_output = FormattedFuturesJson {
                    current_elapsed_ns: elapsed.as_nanos() as u64,
                    futures: futures_json.futures,
                };
                match serde_json::to_string(&json_output) {
                    Ok(json) => println!("{}", json),
                    Err(e) => eprintln!("Failed to serialize statistics to JSON: {}", e),
                }
            }
            Format::JsonPretty => {
                let json_output = FormattedFuturesJson {
                    current_elapsed_ns: elapsed.as_nanos() as u64,
                    futures: futures_json.futures,
                };
                match serde_json::to_string_pretty(&json_output) {
                    Ok(json) => println!("{}", json),
                    Err(e) => eprintln!("Failed to serialize statistics to pretty JSON: {}", e),
                }
            }
        }
    }
}
