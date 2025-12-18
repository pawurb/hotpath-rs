use arc_swap::ArcSwapOption;
use crossbeam_channel::{bounded, select, unbounded};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};
use std::thread;
use std::time::Instant;

use crate::http_server::HTTP_SERVER_PORT;
use crate::output::{FunctionLogEntry, FunctionLogsJson, FunctionsJson, MetricsProvider};
use crate::output_on::{JsonPrettyReporter, JsonReporter, TableReporter};
use crate::Reporter;

use super::{FunctionsQuery, FUNCTIONS_STATE};

cfg_if::cfg_if! {
    if #[cfg(feature = "hotpath-alloc")] {
        use super::alloc::{
            report::{StatsData, TimingStatsData},
            state::{FunctionStats, FunctionsState, Measurement, process_measurement, flush_batch},
        };
    } else {
        use super::timing::{
            report::StatsData,
            state::{FunctionStats, FunctionsState, Measurement, process_measurement, flush_batch},
        };
    }
}

use super::MeasurementGuard;
use crate::Format;

enum ReporterConfig {
    Format(Format),
    Custom(Box<dyn Reporter>),
    None,
}

/// Builder for creating a functions profiling guard with custom configuration.
///
/// `FunctionsGuardBuilder` provides manual control over the profiling lifecycle, allowing you to
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
/// use hotpath::FunctionsGuardBuilder;
///
/// let _guard = FunctionsGuardBuilder::new("my_program").build();
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
/// use hotpath::{FunctionsGuardBuilder, Format};
///
/// let _guard = FunctionsGuardBuilder::new("benchmark")
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
/// use hotpath::{FunctionsGuardBuilder, Reporter, MetricsProvider};
///
/// struct MyReporter;
/// impl Reporter for MyReporter {
///     fn report(&self, metrics: &dyn MetricsProvider<'_>) -> Result<(), Box<dyn std::error::Error>> {
///         // Custom reporting logic
///         Ok(())
///     }
/// }
///
/// let _guard = FunctionsGuardBuilder::new("main")
///     .reporter(Box::new(MyReporter))
///     .build();
/// # }
/// ```
///
/// # Limitations
///
/// Only one hotpath guard can be active at a time. Creating a second guard (either via
/// `FunctionsGuardBuilder` or via the [`main`] macro) will cause a panic.
///
/// # See Also
///
/// * [`main`] - Attribute macro for automatic initialization
/// * [`Format`] - Output format options
/// * [`Reporter`] - Custom reporter trait
#[must_use = "builder is discarded without creating a guard"]
pub struct FunctionsGuardBuilder {
    caller_name: &'static str,
    percentiles: Vec<u8>,
    reporter: ReporterConfig,
    limit: usize,
}

impl FunctionsGuardBuilder {
    /// Creates a new `FunctionsGuardBuilder` with the specified caller name.
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
    /// use hotpath::FunctionsGuardBuilder;
    ///
    /// let _guard = FunctionsGuardBuilder::new("my_program").build();
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
    /// use hotpath::FunctionsGuardBuilder;
    ///
    /// let _guard = FunctionsGuardBuilder::new("main")
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
    /// use hotpath::FunctionsGuardBuilder;
    ///
    /// let _guard = FunctionsGuardBuilder::new("main")
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
    /// use hotpath::{FunctionsGuardBuilder, Format};
    ///
    /// let _guard = FunctionsGuardBuilder::new("main")
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
    /// use hotpath::{FunctionsGuardBuilder, Reporter, MetricsProvider};
    ///
    /// struct CsvReporter;
    /// impl Reporter for CsvReporter {
    ///     fn report(&self, metrics: &dyn MetricsProvider<'_>) -> Result<(), Box<dyn std::error::Error>> {
    ///         // Write metrics to CSV file
    ///         Ok(())
    ///     }
    /// }
    ///
    /// let _guard = FunctionsGuardBuilder::new("main")
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

    /// Builds and initializes the functions profiling guard.
    ///
    /// This method initializes the background profiling thread and returns a guard
    /// that will generate the functions profiling report when dropped.
    ///
    /// # Panics
    ///
    /// Panics if another functions guard is already active. Only one guard can be
    /// active at a time.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # #[cfg(feature = "hotpath")]
    /// # {
    /// use hotpath::FunctionsGuardBuilder;
    ///
    /// let _guard = FunctionsGuardBuilder::new("main").build();
    /// // Profiling is active until _guard is dropped
    /// # }
    /// ```
    pub fn build(self) -> FunctionsGuard {
        let reporter: Box<dyn Reporter> = match self.reporter {
            ReporterConfig::Format(format) => match format {
                Format::Table => Box::new(TableReporter),
                Format::Json => Box::new(JsonReporter),
                Format::JsonPretty => Box::new(JsonPrettyReporter),
            },
            ReporterConfig::Custom(reporter) => reporter,
            ReporterConfig::None => Box::new(TableReporter),
        };

        let recent_logs_limit = std::env::var("HOTPATH_RECENT_LOGS")
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(50);

        FunctionsGuard::new(
            self.caller_name,
            &self.percentiles,
            self.limit,
            reporter,
            recent_logs_limit,
        )
    }

    /// Builds the functions profiling guard and automatically drops it after the specified duration and exits the program.
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
    /// use hotpath::FunctionsGuardBuilder;
    ///
    /// // Profile for 1 second then exit
    /// FunctionsGuardBuilder::new("timed_benchmark")
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

#[must_use = "guard is dropped immediately without generating a report"]
pub struct FunctionsGuard {
    state: Arc<RwLock<FunctionsState>>,
    reporter: Box<dyn Reporter>,
    wrapper_guard: Option<MeasurementGuard>,
}

impl FunctionsGuard {
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
        {
            super::alloc::core::ALLOCATIONS.with(|stack| {
                stack.tracking_enabled.set(false);
            });
            super::alloc::core::init_thread_alloc_tracking();
        }

        let percentiles = percentiles.to_vec();

        let arc_swap = FUNCTIONS_STATE.get_or_init(|| ArcSwapOption::from(None));

        if arc_swap.load().is_some() {
            panic!("More than one _hotpath guard cannot be alive at the same time.");
        }

        let (tx, rx) = unbounded::<Measurement>();
        let (shutdown_tx, shutdown_rx) = bounded::<()>(1);
        let (completion_tx, completion_rx) = bounded::<HashMap<&'static str, FunctionStats>>(1);
        let (query_tx, query_rx) = unbounded::<FunctionsQuery>();
        let start_time = Instant::now();

        let state_arc = Arc::new(RwLock::new(FunctionsState {
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
                                    FunctionsQuery::Alloc(response_tx) => {
                                        cfg_if::cfg_if! {
                                            if #[cfg(feature = "hotpath-alloc")] {
                                                // Create allocation metrics snapshot
                                                use crate::output::MetricsProvider;
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
                                    FunctionsQuery::Timing(response_tx) => {
                                        cfg_if::cfg_if! {
                                            if #[cfg(feature = "hotpath-alloc")] {
                                                // Create timing metrics snapshot
                                                use crate::output::MetricsProvider;
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
                                                use crate::output::MetricsProvider;
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
                                    FunctionsQuery::LogsTiming { function_name, response_tx } => {
                                        let response = if let Some(stats) = local_stats.get(function_name.as_str()) {
                                            cfg_if::cfg_if! {
                                                if #[cfg(feature = "hotpath-alloc")] {
                                                    let logs: Vec<FunctionLogEntry> = stats.recent_logs
                                                        .iter()
                                                        .rev()
                                                        .map(|(_bytes, _count, duration_ns, elapsed, tid, result_log)| FunctionLogEntry {
                                                            value: Some(*duration_ns),
                                                            elapsed_nanos: elapsed.as_nanos() as u64,
                                                            alloc_count: None,
                                                            tid: *tid,
                                                            result: result_log.clone(),
                                                        })
                                                        .collect();
                                                } else {
                                                    let logs: Vec<FunctionLogEntry> = stats.recent_logs
                                                        .iter()
                                                        .rev()
                                                        .map(|(duration_ns, elapsed, tid, result_log)| FunctionLogEntry {
                                                            value: Some(*duration_ns),
                                                            elapsed_nanos: elapsed.as_nanos() as u64,
                                                            alloc_count: None,
                                                            tid: *tid,
                                                            result: result_log.clone(),
                                                        })
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
                                    FunctionsQuery::LogsAlloc { function_name, response_tx } => {
                                        cfg_if::cfg_if! {
                                            if #[cfg(feature = "hotpath-alloc")] {
                                                let response = if let Some(stats) = local_stats.get(function_name.as_str()) {
                                                    let logs: Vec<FunctionLogEntry> = stats.recent_logs
                                                        .iter()
                                                        .rev()
                                                        .map(|(bytes, count, _duration_ns, elapsed, tid, result_log)| FunctionLogEntry {
                                                            value: *bytes,
                                                            elapsed_nanos: elapsed.as_nanos() as u64,
                                                            alloc_count: *count,
                                                            tid: *tid,
                                                            result: result_log.clone(),
                                                        })
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
                                                let _ = function_name;
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
        #[cfg(target_os = "linux")]
        crate::channels::START_TIME.get_or_init(quanta::Instant::now);
        #[cfg(not(target_os = "linux"))]
        crate::channels::START_TIME.get_or_init(std::time::Instant::now);

        crate::http_server::start_metrics_server_once(*HTTP_SERVER_PORT);

        // Override reporter with JsonReporter when HOTPATH_JSON env var is enabled
        let reporter: Box<dyn Reporter> = if std::env::var("HOTPATH_JSON")
            .map(|v| v.eq_ignore_ascii_case("true") || v == "1")
            .unwrap_or(false)
        {
            Box::new(JsonReporter)
        } else {
            _reporter
        };

        let wrapper_guard = MeasurementGuard::build(caller_name, true, false);

        // Re-enable allocation tracking after infrastructure is initialized
        #[cfg(feature = "hotpath-alloc")]
        super::alloc::core::ALLOCATIONS.with(|stack| {
            stack.tracking_enabled.set(true);
        });

        Self {
            state: Arc::clone(&state_arc),
            reporter,
            wrapper_guard: Some(wrapper_guard),
        }
    }
}

impl Drop for FunctionsGuard {
    fn drop(&mut self) {
        let wrapper_guard = self.wrapper_guard.take().unwrap();
        drop(wrapper_guard);

        flush_batch();

        let state: Arc<RwLock<FunctionsState>> = Arc::clone(&self.state);

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

        if let Some(arc_swap) = FUNCTIONS_STATE.get() {
            arc_swap.store(None);
        }
    }
}
