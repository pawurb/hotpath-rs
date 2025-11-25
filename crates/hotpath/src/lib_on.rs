use crate::output;
use crate::output::{FunctionLogsJson, FunctionsJson, MetricsProvider};

#[doc(hidden)]
pub use cfg_if::cfg_if;
pub use hotpath_macros::{main, measure, measure_all, skip};

// Channels module for instrumenting channels
pub mod channels;

// Streams module for instrumenting streams
pub mod streams;

// Threads module for monitoring OS thread metrics
#[cfg(feature = "threads")]
pub mod threads;

pub use channels::{Instrument, InstrumentLog};
pub use streams::{InstrumentStream, InstrumentStreamLog};

use crossbeam_channel::Sender;

/// Query request sent from TUI HTTP server to profiler worker thread
pub enum QueryRequest {
    /// Request full metrics snapshot (allocation metrics) - returns None if hotpath-alloc not enabled
    GetFunctions(Sender<Option<FunctionsJson>>),
    /// Request timing metrics snapshot
    GetFunctionsTiming(Sender<FunctionsJson>),
    /// Request timing function logs for a specific function (returns None if function not found)
    GetFunctionLogsTiming {
        function_name: String,
        response_tx: Sender<Option<FunctionLogsJson>>,
    },
    /// Request allocation function logs for a specific function (returns None if hotpath-alloc not enabled or function not found)
    GetFunctionLogsAlloc {
        function_name: String,
        response_tx: Sender<Option<FunctionLogsJson>>,
    },
}

cfg_if::cfg_if! {
    if #[cfg(feature = "hotpath-alloc")] {
        mod alloc;
        #[doc(hidden)]
        pub use tokio::runtime::{Handle, RuntimeFlavor};

        // Memory allocations profiling using a custom global allocator
        #[global_allocator]
        static GLOBAL: alloc::allocator::CountingAllocator = alloc::allocator::CountingAllocator {};

        pub use alloc::guard::MeasurementGuard;
        pub use alloc::state::FunctionStats;
        use alloc::{
            report::{StatsData, TimingStatsData},
            state::{HotPathState, Measurement, process_measurement},
        };
    } else {
        // Time-based profiling (when no allocation features are enabled)
        mod time;
        pub use time::guard::MeasurementGuard;
        pub use time::state::FunctionStats;
        use time::{
            report::StatsData,
            state::{HotPathState, Measurement, process_measurement},
        };
    }
}

impl MeasurementGuard {
    pub fn build(measurement_name: &'static str, wrapper: bool, _is_async: bool) -> Self {
        #[allow(clippy::needless_bool)]
        let unsupported_async = if wrapper {
            // top wrapper functions are not inside a runtime
            false
        } else {
            cfg_if::cfg_if! {
                if #[cfg(feature = "hotpath-alloc")] {
                    // For allocation profiling: mark async as unsupported unless
                    // running on Tokio CurrentThread. Non-Tokio runtimes are unsupported.
                    if _is_async {
                        match Handle::try_current() {
                            Ok(h) => h.runtime_flavor() != RuntimeFlavor::CurrentThread,
                            Err(_) => true,
                        }
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
        };

        MeasurementGuard::new(measurement_name, wrapper, unsupported_async)
    }
}

/// Output format for profiling reports.
///
/// This enum specifies how profiling results should be displayed when the program exits.
///
/// # Variants
///
/// * `Table` - Human-readable table format (default)
/// * `Json` - Compact JSON format (single line)
/// * `JsonPretty` - Pretty-printed JSON format with indentation
///
/// # Examples
///
/// ```rust
/// # #[cfg(feature = "hotpath")]
/// # {
/// use hotpath::{GuardBuilder, Format};
///
/// let _guard = GuardBuilder::new("main")
///     .format(Format::JsonPretty)
///     .build();
/// # }
/// ```
#[derive(Clone, Copy, Debug, Default)]
pub enum Format {
    #[default]
    Table,
    Json,
    JsonPretty,
}

use crossbeam_channel::{bounded, select, unbounded};
use std::collections::HashMap;
use std::thread;
use std::time::Instant;

/// Measures the execution time or memory allocations of a code block.
///
/// This macro wraps a block of code with profiling instrumentation, similar to the
/// [`measure`](hotpath_macros::measure) attribute macro but for inline code blocks.
/// The block is labeled with a static string identifier.
///
/// # Arguments
///
/// * `$label` - A static string label to identify this code block in the profiling report
/// * `$expr` - The expression or code block to measure
///
/// # Behavior
///
/// The macro automatically uses the appropriate measurement based on enabled feature flags:
/// - **Time profiling** (default): Measures execution duration
/// - **Allocation profiling**: Tracks memory allocations when allocation features are enabled
///
/// # Examples
///
/// ```rust
/// # #[cfg(feature = "hotpath")]
/// # {
/// use std::time::Duration;
///
/// #[cfg(feature = "hotpath")]
/// hotpath::measure_block!("data_processing", {
///     // Your code here
///     std::thread::sleep(Duration::from_millis(10));
/// });
/// # }
/// ```
///
/// # See Also
///
/// * [`measure`](hotpath_macros::measure) - Attribute macro for instrumenting functions
/// * [`main`](hotpath_macros::main) - Attribute macro that initializes profiling
#[cfg(feature = "hotpath")]
#[macro_export]
macro_rules! measure_block {
    ($label:expr, $expr:expr) => {{
        let _guard = hotpath::MeasurementGuard::new($label, false, false);

        $expr
    }};
}

#[cfg(not(feature = "hotpath"))]
#[macro_export]
macro_rules! measure_block {
    ($label:expr, $expr:expr) => {{
        $expr
    }};
}

use arc_swap::ArcSwapOption;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::OnceLock;
use std::sync::RwLock;

use crate::Reporter;

pub(crate) static HOTPATH_STATE: OnceLock<ArcSwapOption<RwLock<HotPathState>>> = OnceLock::new();

/// Builder for creating a hotpath profiling guard with custom configuration.
///
/// `GuardBuilder` provides manual control over the profiling lifecycle, allowing you to
/// start and stop profiling at specific points in your code. The profiling report is
/// generated when the guard is dropped.
///
/// # Examples
///
/// Basic usage with default settings:
///
/// ```rust
/// # #[cfg(feature = "hotpath")]
/// # {
/// use hotpath::GuardBuilder;
///
/// let _guard = GuardBuilder::new("my_program").build();
/// // Your code here - measurements will be collected
/// // Report is printed when _guard goes out of scope
/// # }
/// ```
///
/// Custom configuration:
///
/// ```rust
/// # #[cfg(feature = "hotpath")]
/// # {
/// use hotpath::{GuardBuilder, Format};
///
/// let _guard = GuardBuilder::new("benchmark")
///     .percentiles(&[50, 90, 95, 99])
///     .format(Format::JsonPretty)
///     .build();
/// # }
/// ```
///
/// With custom reporter:
///
/// ```rust
/// # #[cfg(feature = "hotpath")]
/// # {
/// use hotpath::{GuardBuilder, Reporter, MetricsProvider};
///
/// struct MyReporter;
/// impl Reporter for MyReporter {
///     fn report(&self, metrics: &dyn MetricsProvider<'_>) -> Result<(), Box<dyn std::error::Error>> {
///         // Custom reporting logic
///         Ok(())
///     }
/// }
///
/// let _guard = GuardBuilder::new("main")
///     .reporter(Box::new(MyReporter))
///     .build();
/// # }
/// ```
///
/// # Limitations
///
/// Only one hotpath guard can be active at a time. Creating a second guard (either via
/// `GuardBuilder` or via the [`main`] macro) will cause a panic.
///
/// # See Also
///
/// * [`main`] - Attribute macro for automatic initialization
/// * [`Format`] - Output format options
/// * [`Reporter`] - Custom reporter trait
pub struct GuardBuilder {
    caller_name: &'static str,
    percentiles: Vec<u8>,
    reporter: ReporterConfig,
    limit: usize,
}

enum ReporterConfig {
    Format(Format),
    Custom(Box<dyn Reporter>),
    None, // Will default to Format::Table
}

impl GuardBuilder {
    /// Creates a new `GuardBuilder` with the specified caller name.
    ///
    /// The caller name is used to identify the profiling session in the report.
    ///
    /// # Arguments
    ///
    /// * `caller_name` - A string identifier for this profiling session (e.g., "main", "benchmark")
    ///
    /// # Examples
    ///
    /// ```rust
    /// # #[cfg(feature = "hotpath")]
    /// # {
    /// use hotpath::GuardBuilder;
    ///
    /// let _guard = GuardBuilder::new("my_program").build();
    /// # }
    /// ```
    pub fn new(caller_name: &'static str) -> Self {
        Self {
            caller_name,
            percentiles: vec![95],
            reporter: ReporterConfig::None,
            limit: 15,
        }
    }

    /// Sets the percentiles to display in the profiling report.
    ///
    /// Percentiles help identify performance distribution patterns across multiple
    /// measurements of the same function. Valid values are 0-100, where 0 represents
    /// the minimum value and 100 represents the maximum.
    ///
    /// Default: `[95]`
    ///
    /// # Arguments
    ///
    /// * `percentiles` - Slice of percentile values (0-100) to display
    ///
    /// # Examples
    ///
    /// ```rust
    /// # #[cfg(feature = "hotpath")]
    /// # {
    /// use hotpath::GuardBuilder;
    ///
    /// let _guard = GuardBuilder::new("main")
    ///     .percentiles(&[50, 90, 95, 99])
    ///     .build();
    /// # }
    /// ```
    pub fn percentiles(mut self, percentiles: &[u8]) -> Self {
        self.percentiles = percentiles.to_vec();
        self
    }

    /// Sets the maximum number of functions to display in the profiling report.
    ///
    /// The report will show only the top N functions sorted by total execution time
    /// (or total allocations when using allocation profiling features).
    ///
    /// Default: `15`
    ///
    /// # Arguments
    ///
    /// * `limit` - Maximum number of functions to display (0 means show all)
    ///
    /// # Examples
    ///
    /// ```rust
    /// # #[cfg(feature = "hotpath")]
    /// # {
    /// use hotpath::GuardBuilder;
    ///
    /// let _guard = GuardBuilder::new("main")
    ///     .limit(20)
    ///     .build();
    /// # }
    /// ```
    pub fn limit(mut self, limit: usize) -> Self {
        self.limit = limit;
        self
    }

    /// Sets the output format for the profiling report.
    ///
    /// # Arguments
    ///
    /// * `format` - The output format (Table, Json, or JsonPretty)
    ///
    /// # Examples
    ///
    /// ```rust
    /// # #[cfg(feature = "hotpath")]
    /// # {
    /// use hotpath::{GuardBuilder, Format};
    ///
    /// let _guard = GuardBuilder::new("main")
    ///     .format(Format::JsonPretty)
    ///     .build();
    /// # }
    /// ```
    ///
    /// # See Also
    ///
    /// * [`Format`] - Available output formats
    pub fn format(mut self, format: Format) -> Self {
        self.reporter = ReporterConfig::Format(format);
        self
    }

    /// Sets a custom reporter for the profiling report.
    ///
    /// Custom reporters allow you to control how profiling results are handled,
    /// enabling integration with logging systems, CI pipelines, or monitoring tools.
    ///
    /// When a custom reporter is set, it overrides any format setting.
    ///
    /// # Arguments
    ///
    /// * `reporter` - A boxed implementation of the [`Reporter`] trait
    ///
    /// # Examples
    ///
    /// ```rust
    /// # #[cfg(feature = "hotpath")]
    /// # {
    /// use hotpath::{GuardBuilder, Reporter, MetricsProvider};
    ///
    /// struct CsvReporter;
    /// impl Reporter for CsvReporter {
    ///     fn report(&self, metrics: &dyn MetricsProvider<'_>) -> Result<(), Box<dyn std::error::Error>> {
    ///         // Write metrics to CSV file
    ///         Ok(())
    ///     }
    /// }
    ///
    /// let _guard = GuardBuilder::new("main")
    ///     .reporter(Box::new(CsvReporter))
    ///     .build();
    /// # }
    /// ```
    ///
    /// # See Also
    ///
    /// * [`Reporter`] - Reporter trait for custom implementations
    pub fn reporter(mut self, reporter: Box<dyn Reporter>) -> Self {
        self.reporter = ReporterConfig::Custom(reporter);
        self
    }

    /// Builds and initializes the hotpath profiling guard.
    ///
    /// This method initializes the background profiling thread and returns a guard
    /// that will generate the profiling report when dropped.
    ///
    /// # Panics
    ///
    /// Panics if another hotpath guard is already active. Only one guard can be
    /// active at a time.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # #[cfg(feature = "hotpath")]
    /// # {
    /// use hotpath::GuardBuilder;
    ///
    /// let _guard = GuardBuilder::new("main").build();
    /// // Profiling is active until _guard is dropped
    /// # }
    /// ```
    pub fn build(self) -> HotPath {
        let reporter: Box<dyn Reporter> = match self.reporter {
            ReporterConfig::Format(format) => match format {
                Format::Table => Box::new(output::TableReporter),
                Format::Json => Box::new(output::JsonReporter),
                Format::JsonPretty => Box::new(output::JsonPrettyReporter),
            },
            ReporterConfig::Custom(reporter) => reporter,
            ReporterConfig::None => Box::new(output::TableReporter),
        };

        let recent_logs_limit = std::env::var("HOTPATH_RECENT_LOGS")
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(50);

        HotPath::new(
            self.caller_name,
            &self.percentiles,
            self.limit,
            reporter,
            recent_logs_limit,
        )
    }

    /// Builds the hotpath profiling guard and automatically drops it after the specified duration and exits the program.
    ///
    /// If used in memory profiling mode, it disables the top level measurement. To support timeout guard is moved between threads making accurate memory measurements impossible.
    /// # Arguments
    ///
    /// * `duration` - The duration to wait before dropping the guard and generating the report
    ///
    /// # Examples
    ///
    /// ```rust
    /// # #[cfg(feature = "hotpath")]
    /// # {
    /// use std::time::Duration;
    /// use hotpath::GuardBuilder;
    ///
    /// // Profile for 1 second then exit
    /// GuardBuilder::new("timed_benchmark")
    ///     .build_with_timeout(Duration::from_secs(1));
    ///
    /// // Your code here - will be profiled for 1 second
    /// loop {
    ///     // Work...
    /// }
    /// # }
    /// ```
    pub fn build_with_timeout(self, duration: std::time::Duration) {
        let guard = self.build();
        thread::spawn(move || {
            thread::sleep(duration);
            drop(guard);
            std::process::exit(0);
        });
    }
}

impl HotPath {
    pub fn new(
        caller_name: &'static str,
        percentiles: &[u8],
        limit: usize,
        _reporter: Box<dyn Reporter>,
        recent_logs_limit: usize,
    ) -> Self {
        // Disable allocation tracking during infrastructure initialization
        // to prevent profiling overhead from being included in measurements
        #[cfg(feature = "hotpath-alloc")]
        alloc::core::ALLOCATIONS.with(|stack| {
            stack.tracking_enabled.set(false);
        });

        let percentiles = percentiles.to_vec();

        let arc_swap = HOTPATH_STATE.get_or_init(|| ArcSwapOption::from(None));

        if arc_swap.load().is_some() {
            panic!("More than one _hotpath guard cannot be alive at the same time.");
        }

        let (tx, rx) = unbounded::<Measurement>();
        let (shutdown_tx, shutdown_rx) = bounded::<()>(1);
        let (completion_tx, completion_rx) = bounded::<HashMap<&'static str, FunctionStats>>(1);
        let (query_tx, query_rx) = unbounded::<QueryRequest>();
        let start_time = Instant::now();

        let state_arc = Arc::new(RwLock::new(HotPathState {
            sender: Some(tx),
            shutdown_tx: Some(shutdown_tx),
            completion_rx: Some(Mutex::new(completion_rx)),
            query_tx: Some(query_tx),
            start_time,
            caller_name,
            percentiles: percentiles.clone(),
            limit,
        }));

        let worker_start_time = start_time;
        let worker_percentiles = percentiles.clone();
        let worker_caller_name = caller_name;
        let worker_limit = limit;
        let worker_recent_logs_limit = recent_logs_limit;

        thread::Builder::new()
            .name("hp-worker".into())
            .spawn(move || {
                let mut local_stats = HashMap::<&'static str, FunctionStats>::new();

                loop {
                    select! {
                        recv(rx) -> result => {
                            match result {
                                Ok(measurement) => {
                                    process_measurement(&mut local_stats, measurement, worker_recent_logs_limit);
                                }
                                Err(_) => break, // Channel disconnected
                            }
                        }
                        recv(shutdown_rx) -> _ => {
                            // Process remaining messages after shutdown signal
                            while let Ok(measurement) = rx.try_recv() {
                                process_measurement(&mut local_stats, measurement, worker_recent_logs_limit);
                            }
                            break;
                        }
                        recv(query_rx) -> result => {
                            if let Ok(query_request) = result {
                                match query_request {
                                    QueryRequest::GetFunctions(response_tx) => {
                                        cfg_if::cfg_if! {
                                            if #[cfg(feature = "hotpath-alloc")] {
                                                // Create allocation metrics snapshot
                                                use output::MetricsProvider;
                                                let total_elapsed = worker_start_time.elapsed();
                                                let metrics_provider = StatsData::new(
                                                    &local_stats,
                                                    total_elapsed,
                                                    worker_percentiles.clone(),
                                                    worker_caller_name,
                                                    worker_limit,
                                                );
                                                let metrics_json = FunctionsJson::from(&metrics_provider as &dyn MetricsProvider);
                                                let _ = response_tx.send(Some(metrics_json));
                                            } else {
                                                // Allocation profiling not available without hotpath-alloc feature
                                                let _ = response_tx.send(None);
                                            }
                                        }
                                    }
                                    QueryRequest::GetFunctionsTiming(response_tx) => {
                                        cfg_if::cfg_if! {
                                            if #[cfg(feature = "hotpath-alloc")] {
                                                // Create timing metrics snapshot
                                                use output::MetricsProvider;
                                                let total_elapsed = worker_start_time.elapsed();
                                                let metrics_provider = TimingStatsData::new(
                                                    &local_stats,
                                                    total_elapsed,
                                                    worker_percentiles.clone(),
                                                    worker_caller_name,
                                                    worker_limit,
                                                );
                                                let metrics_json = FunctionsJson::from(&metrics_provider as &dyn MetricsProvider);
                                                let _ = response_tx.send(metrics_json);
                                            } else {
                                                // For time-only mode, GetTimingFunctions returns the same as GetFunctions
                                                use output::MetricsProvider;
                                                let total_elapsed = worker_start_time.elapsed();
                                                let metrics_provider = StatsData::new(
                                                    &local_stats,
                                                    total_elapsed,
                                                    worker_percentiles.clone(),
                                                    worker_caller_name,
                                                    worker_limit,
                                                );
                                                let metrics_json = FunctionsJson::from(&metrics_provider as &dyn MetricsProvider);
                                                let _ = response_tx.send(metrics_json);
                                            }
                                        }
                                    }
                                    QueryRequest::GetFunctionLogsTiming { function_name, response_tx } => {
                                        let response = if let Some(stats) = local_stats.get(function_name.as_str()) {
                                            cfg_if::cfg_if! {
                                                if #[cfg(feature = "hotpath-alloc")] {
                                                    let logs: Vec<(Option<u64>, u64, Option<u64>, u64)> = stats.recent_logs
                                                        .iter()
                                                        .rev()
                                                        .map(|(_bytes, _count, duration_ns, elapsed, tid)| (Some(*duration_ns), elapsed.as_nanos() as u64, None, *tid))
                                                        .collect();
                                                } else {
                                                    let logs: Vec<(Option<u64>, u64, Option<u64>, u64)> = stats.recent_logs
                                                        .iter()
                                                        .rev()
                                                        .map(|(duration_ns, elapsed, tid)| (Some(*duration_ns), elapsed.as_nanos() as u64, None, *tid))
                                                        .collect();
                                                }
                                            }
                                            Some(FunctionLogsJson {
                                                function_name: function_name.clone(),
                                                logs,
                                                count: stats.count as usize,
                                            })
                                        } else {
                                            // Function not found
                                            None
                                        };
                                        let _ = response_tx.send(response);
                                    }
                                    QueryRequest::GetFunctionLogsAlloc { function_name, response_tx } => {
                                        cfg_if::cfg_if! {
                                            if #[cfg(feature = "hotpath-alloc")] {
                                                let response = if let Some(stats) = local_stats.get(function_name.as_str()) {
                                                    let logs: Vec<(Option<u64>, u64, Option<u64>, u64)> = stats.recent_logs
                                                        .iter()
                                                        .rev()
                                                        .map(|(bytes, count, _duration_ns, elapsed, tid)| (*bytes, elapsed.as_nanos() as u64, *count, *tid))
                                                        .collect();
                                                    Some(FunctionLogsJson {
                                                        function_name,
                                                        logs,
                                                        count: stats.count as usize, // Total invocations, not just recent logs
                                                    })
                                                } else {
                                                    None
                                                };
                                                let _ = response_tx.send(response);
                                            } else {
                                                // Return None if hotpath-alloc feature is not enabled
                                                let _ = function_name; // Suppress unused variable warning
                                                let _ = response_tx.send(None);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                // Send stats via completion channel
                let _ = completion_tx.send(local_stats);
            })
            .expect("Failed to spawn hotpath-worker thread");

        arc_swap.store(Some(Arc::clone(&state_arc)));

        // Initialize START_TIME for channels/streams (required before HTTP server starts)
        crate::channels::START_TIME.get_or_init(std::time::Instant::now);

        // Start HTTP metrics server (default port 6770, customizable via HOTPATH_HTTP_PORT)
        let port = std::env::var("HOTPATH_HTTP_PORT")
            .ok()
            .and_then(|p| p.parse::<u16>().ok())
            .unwrap_or(6770);
        crate::http_server::start_metrics_server_once(port);

        // Override reporter with JsonReporter when HOTPATH_JSON env var is enabled
        let reporter: Box<dyn Reporter> = if std::env::var("HOTPATH_JSON")
            .map(|v| v.eq_ignore_ascii_case("true") || v == "1")
            .unwrap_or(false)
        {
            Box::new(output::JsonReporter)
        } else {
            _reporter
        };

        let wrapper_guard = MeasurementGuard::build(caller_name, true, false);

        // Re-enable allocation tracking after infrastructure is initialized
        #[cfg(feature = "hotpath-alloc")]
        alloc::core::ALLOCATIONS.with(|stack| {
            stack.tracking_enabled.set(true);
        });

        Self {
            state: Arc::clone(&state_arc),
            reporter,
            wrapper_guard: Some(wrapper_guard),
        }
    }
}

pub struct HotPath {
    state: Arc<RwLock<HotPathState>>,
    reporter: Box<dyn Reporter>,
    wrapper_guard: Option<MeasurementGuard>,
}

impl Drop for HotPath {
    fn drop(&mut self) {
        let wrapper_guard = self.wrapper_guard.take().unwrap();
        drop(wrapper_guard);

        let state: Arc<RwLock<HotPathState>> = Arc::clone(&self.state);

        // Signal shutdown and wait for processing thread to complete
        let (shutdown_tx, completion_rx, end_time) = {
            let Ok(mut state_guard) = state.write() else {
                return;
            };

            state_guard.sender = None;
            let end_time = Instant::now();

            let shutdown_tx = state_guard.shutdown_tx.take();
            let completion_rx = state_guard.completion_rx.take();
            (shutdown_tx, completion_rx, end_time)
        };

        if let Some(tx) = shutdown_tx {
            let _ = tx.send(());
        }

        if let Some(rx_mutex) = completion_rx {
            if let Ok(rx) = rx_mutex.lock() {
                if let Ok(stats) = rx.recv() {
                    if let Ok(state_guard) = state.read() {
                        let total_elapsed = end_time.duration_since(state_guard.start_time);
                        let metrics_provider = StatsData::new(
                            &stats,
                            total_elapsed,
                            state_guard.percentiles.clone(),
                            state_guard.caller_name,
                            state_guard.limit,
                        );

                        match self.reporter.report(&metrics_provider) {
                            Ok(()) => (),
                            Err(e) => eprintln!("Failed to report hotpath metrics: {}", e),
                        }
                    }
                }
            }
        }

        if let Some(arc_swap) = HOTPATH_STATE.get() {
            arc_swap.store(None);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn is_send_sync<T: Send + Sync>() {}

    #[test]
    fn test_hotpath_is_send_sync() {
        is_send_sync::<HotPath>();
    }
}
