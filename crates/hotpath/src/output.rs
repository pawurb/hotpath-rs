use serde::ser::Serializer;
use serde::{Deserialize, Serialize};
#[cfg(feature = "hotpath")]
use std::collections::HashMap;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
#[cfg(feature = "hotpath")]
use std::time::Duration;

#[cfg(feature = "hotpath")]
use crate::FunctionStats;

pub use crate::shared::{
    format_bytes, format_duration, MetricType, OutputDestination, ProfilingMode,
};

impl OutputDestination {
    /// Creates a writer for this destination.
    ///
    /// Returns a boxed writer that implements `Write`.
    /// For `Stdout`, returns a handle to stdout.
    /// For `File`, creates parent directories if needed, then creates or truncates the file.
    pub fn writer(&self) -> Result<Box<dyn Write>, std::io::Error> {
        match self {
            OutputDestination::Stdout => Ok(Box::new(std::io::stdout())),
            OutputDestination::File(path) => {
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                Ok(Box::new(File::create(path)?))
            }
        }
    }

    /// Creates an OutputDestination from an optional path.
    ///
    /// Environment variable `HOTPATH_OUTPUT_PATH` takes precedence over programmatic config.
    /// If the path is provided, resolves relative paths against the current working directory.
    /// If no path is provided, returns Stdout.
    pub fn from_path(path: Option<PathBuf>) -> Self {
        if let Ok(env_path) = std::env::var("HOTPATH_OUTPUT_PATH") {
            return OutputDestination::File(resolve_output_path(env_path));
        }

        match path {
            Some(p) => OutputDestination::File(p),
            None => OutputDestination::Stdout,
        }
    }
}

/// Resolves a path, converting relative paths to absolute by joining with cwd.
pub fn resolve_output_path(path: impl AsRef<std::path::Path>) -> PathBuf {
    let path = path.as_ref();
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map(|cwd| cwd.join(path))
            .unwrap_or_else(|_| path.to_path_buf())
    }
}

impl Serialize for MetricType {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            MetricType::CallsCount(count) => serializer.serialize_u64(*count),
            MetricType::DurationNs(ns) => serializer.serialize_u64(*ns),
            MetricType::Alloc(bytes, _count) => serializer.serialize_u64(*bytes),
            MetricType::Percentage(basis_points) => serializer.serialize_u64(*basis_points),
            MetricType::Unsupported => serializer.serialize_none(),
        }
    }
}

impl Serialize for ProfilingMode {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            ProfilingMode::Timing => serializer.serialize_str("timing"),
            ProfilingMode::Alloc => serializer.serialize_str("alloc"),
        }
    }
}

impl<'de> Deserialize<'de> for ProfilingMode {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        match s.as_str() {
            "timing" => Ok(ProfilingMode::Timing),
            "alloc" => Ok(ProfilingMode::Alloc),
            _ => Err(serde::de::Error::unknown_variant(&s, &["timing", "alloc"])),
        }
    }
}

/// Find the nearest valid char boundary at or before `index`.
/// Used to safely truncate UTF-8 strings from the right.
pub fn floor_char_boundary(s: &str, index: usize) -> usize {
    s.char_indices()
        .map(|(i, _)| i)
        .take_while(|&i| i <= index)
        .last()
        .unwrap_or(0)
}

/// Find the nearest valid char boundary at or after `index`.
/// Used to safely truncate UTF-8 strings from the left.
pub fn ceil_char_boundary(s: &str, index: usize) -> usize {
    s.char_indices()
        .map(|(i, _)| i)
        .find(|&i| i >= index)
        .unwrap_or(s.len())
}

pub const MAX_RESULT_LEN: usize = 1536;

/// Truncate a result string to MAX_RESULT_LEN, respecting UTF-8 char boundaries.
pub fn truncate_result(s: String) -> String {
    if s.len() <= MAX_RESULT_LEN {
        s
    } else {
        let end = floor_char_boundary(&s, MAX_RESULT_LEN.saturating_sub(3));
        format!("{}...", &s[..end])
    }
}

pub fn shorten_function_name(function_name: &str) -> String {
    let parts: Vec<&str> = function_name.split("::").collect();
    if parts.len() > 2 {
        parts[parts.len() - 2..].join("::")
    } else {
        function_name.to_string()
    }
}

/// Trait for implementing custom profiling report output.
///
/// Implement this trait to control how profiling results are displayed or stored.
/// Custom reporters can integrate hotpath with logging systems, CI pipelines,
/// monitoring tools, or custom file formats.
///
/// # Examples
///
/// ```rust
/// use hotpath::{Reporter, MetricsProvider, OutputDestination};
/// use std::error::Error;
///
/// struct SimpleLogger;
///
/// impl Reporter for SimpleLogger {
///     fn report(
///         &self,
///         metrics: &dyn MetricsProvider<'_>,
///         _output: &OutputDestination,
///     ) -> Result<(), Box<dyn Error>> {
///         println!("Profiling {} complete", metrics.caller_name());
///         println!("Functions measured: {}", metrics.metric_data().len());
///         Ok(())
///     }
/// }
/// ```
///
/// # See Also
///
/// * [`MetricsProvider`] - Trait for accessing profiling metrics data
/// * `FunctionsGuardBuilder::reporter` - Method to set custom reporter
pub trait Reporter: Send + Sync {
    /// Generate a report to the specified output destination.
    fn report(
        &self,
        metrics_provider: &dyn MetricsProvider<'_>,
        output: &OutputDestination,
    ) -> Result<(), Box<dyn std::error::Error>>;
}

/// A single log entry for a function invocation.
///
/// - For timing mode: `value` is duration in nanoseconds, `alloc_count` is None
/// - For alloc mode with valid data: `value` is bytes allocated, `alloc_count` is allocation count
/// - For alloc mode with invalid data: `value` and `alloc_count` are None (cross-thread or unsupported async)
/// - `tid` is None if cross-thread execution was detected
/// - `result` contains the Debug representation of the return value when `log = true`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionLog {
    /// Measured value (duration in ns for timing, bytes for memory). None if invalid.
    pub value: Option<u64>,
    /// Timestamp when the measurement was taken (nanoseconds since profiler start)
    pub elapsed_nanos: u64,
    /// Allocation count (only for memory mode)
    pub alloc_count: Option<u64>,
    /// Thread ID where the function was executed, None if cross-thread execution
    pub tid: Option<u64>,
    /// Debug representation of the return value (when log = true)
    pub result: Option<String>,
}

/// Response containing recent logs for a function
#[derive(Debug, Clone)]
pub struct FunctionLogsList {
    pub function_name: String,
    pub logs: Vec<FunctionLog>,
    /// Total number of times this function was invoked (used to calculate invocation numbers)
    pub count: usize,
}

/// Structured per-function profiling metrics data as an ordered list.
pub type FunctionsData = Vec<(String, Vec<MetricType>)>;

/// Trait for accessing profiling metrics data from custom reporters.
///
/// This trait provides a standardized interface for reporters to access profiling
/// metrics, regardless of the underlying profiling mode (time or allocation tracking).
/// Implement [`Reporter`] to use this interface for custom output.
///
/// # See Also
///
/// * [`Reporter`] - Trait for implementing custom reporters
/// * [`MetricType`] - Metric value types
pub trait MetricsProvider<'a> {
    fn description(&self) -> String;
    fn profiling_mode(&self) -> ProfilingMode;
    fn headers(&self) -> Vec<String> {
        let mut headers = vec![
            "Function".to_string(),
            "Calls".to_string(),
            "Avg".to_string(),
        ];

        for &p in &self.percentiles() {
            headers.push(format!("P{}", p));
        }

        headers.push("Total".to_string());
        headers.push("% Total".to_string());

        headers
    }
    fn percentiles(&self) -> Vec<u8>;

    fn metric_data(&self) -> Vec<(String, Vec<MetricType>)>;

    fn sort_key(&self, metrics: &[MetricType]) -> f64 {
        if let Some(MetricType::Percentage(basis_points)) = metrics.last() {
            *basis_points as f64 / 100.0
        } else {
            0.0
        }
    }

    fn has_unsupported_async(&self) -> bool {
        false
    }

    fn entry_counts(&self) -> (usize, usize);

    #[cfg(feature = "hotpath")]
    fn new(
        stats: &'a HashMap<&'static str, FunctionStats>,
        total_elapsed: Duration,
        percentiles: Vec<u8>,
        caller_name: &'static str,
        limit: usize,
    ) -> Self
    where
        Self: Sized;

    fn total_elapsed(&self) -> u64;

    fn caller_name(&self) -> &str;
}

#[cfg(test)]
mod truncation_tests {
    use super::*;

    #[test]
    fn test_truncate_result() {
        let truncate_point = MAX_RESULT_LEN.saturating_sub(3);

        let test_cases: Vec<(&str, String)> = vec![
            (
                "japanese at boundary",
                format!("{}リプライ", "a".repeat(truncate_point - 2)),
            ),
            ("emoji", "🦀".repeat(500)),
            ("chinese", "拥抱中文字符测试".repeat(200)),
            (
                "2-byte at boundary",
                format!("{}ñoño", "a".repeat(truncate_point - 1)),
            ),
        ];

        for (name, input) in test_cases {
            let result = truncate_result(input.clone());
            assert!(
                result.chars().count() > 0,
                "{}: result should have chars",
                name
            );
            if input.len() > MAX_RESULT_LEN {
                assert!(
                    result.ends_with("..."),
                    "{}: truncated result should end with '...'",
                    name
                );
            }
        }
    }
}
