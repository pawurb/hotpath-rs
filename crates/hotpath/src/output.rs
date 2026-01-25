use serde::{ser::Serializer, Deserialize, Serialize};
#[cfg(feature = "hotpath")]
use std::collections::HashMap;
use std::fmt;
#[cfg(feature = "hotpath")]
use std::time::Duration;

#[cfg(feature = "hotpath")]
use crate::FunctionStats;

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

/// Represents different types of profiling metrics with their values.
///
/// This enum wraps metric values with type information, allowing the reporting
/// system to format and display them appropriately. Values are stored in their
/// raw form and formatted when displayed.
///
/// # Variants
///
/// * `CallsCount(u64)` - Number of function calls
/// * `DurationNs(u64)` - Duration in nanoseconds (formatted as human-readable time)
/// * `AllocBytes(u64)` - Bytes allocated (formatted with KB/MB/GB units)
/// * `AllocCount(u64)` - Allocation count
/// * `Percentage(u64)` - Percentage as basis points (1% = 100, formatted as percentage)
/// * `Unsupported` - For N/A values (e.g., async functions when allocation profiling not supported)
///
/// # Examples
///
/// ```rust
/// use hotpath::MetricType;
///
/// let duration = MetricType::DurationNs(1_500_000); // 1.5ms
/// let memory = MetricType::AllocBytes(2048); // 2KB
/// let percent = MetricType::Percentage(9500); // 95.00%
///
/// println!("{}", duration); // Displays: "1.50ms"
/// println!("{}", memory);   // Displays: "2.0 KB"
/// println!("{}", percent);  // Displays: "95.00%"
/// ```
#[derive(Debug, Clone)]
pub enum MetricType {
    CallsCount(u64), // Number of function calls
    DurationNs(u64), // Duration in nanoseconds
    Alloc(u64, u64), // Bytes allocated, objects allocated
    Percentage(u64), // Percentage as basis points (1% = 100)
    Unsupported,     // For N/A values (async functions when not supported)
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

impl fmt::Display for MetricType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MetricType::CallsCount(count) => {
                write!(f, "{}", count)
            }
            MetricType::DurationNs(ns) => {
                write!(f, "{}", format_duration(*ns))
            }
            MetricType::Alloc(bytes, _count) => {
                write!(f, "{}", format_bytes(*bytes))
            }
            MetricType::Percentage(basis_points) => {
                write!(f, "{:.2}%", *basis_points as f64 / 100.0)
            }
            MetricType::Unsupported => {
                write!(f, "N/A*")
            }
        }
    }
}

/// Formats a duration in nanoseconds into a human-readable string with appropriate units.
pub fn format_duration(ns: u64) -> String {
    if ns < 1_000 {
        format!("{} ns", ns)
    } else if ns < 1_000_000 {
        format!("{:.2} µs", ns as f64 / 1_000.0)
    } else if ns < 1_000_000_000 {
        format!("{:.2} ms", ns as f64 / 1_000_000.0)
    } else {
        format!("{:.2} s", ns as f64 / 1_000_000_000.0)
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
/// use hotpath::{Reporter, MetricsProvider};
/// use std::error::Error;
///
/// struct SimpleLogger;
///
/// impl Reporter for SimpleLogger {
///     fn report(&self, metrics: &dyn MetricsProvider<'_>) -> Result<(), Box<dyn Error>> {
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
    fn report(
        &self,
        metrics_provider: &dyn MetricsProvider<'_>,
    ) -> Result<(), Box<dyn std::error::Error>>;
}

/// Profiling mode indicating what type of measurements were collected.
///
/// This enum identifies which profiling feature was active when measurements
/// were collected. It's included in JSON output to help interpret the metrics.
///
/// # Variants
///
/// * `Timing` - Time-based profiling (execution duration)
/// * `Alloc` - Combined allocation profiling (both bytes and count)
#[allow(dead_code)]
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "kebab-case")]
pub enum ProfilingMode {
    Timing,
    Alloc,
}

impl fmt::Display for ProfilingMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProfilingMode::Timing => write!(f, "timing"),
            ProfilingMode::Alloc => write!(f, "alloc"),
        }
    }
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

/// Formats a byte count into a human-readable string (e.g., "1.5 MB").
pub fn format_bytes(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    const THRESHOLD: f64 = 1024.0;

    if bytes == 0 {
        return "0 B".to_string();
    }

    let bytes_f = bytes as f64;
    let unit_index = (bytes_f.log(THRESHOLD).floor() as usize).min(UNITS.len() - 1);
    let unit_value = bytes_f / THRESHOLD.powi(unit_index as i32);

    if unit_index == 0 {
        format!("{} {}", bytes, UNITS[unit_index])
    } else {
        format!("{:.1} {}", unit_value, UNITS[unit_index])
    }
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
