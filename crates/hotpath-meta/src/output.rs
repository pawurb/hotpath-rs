use serde::ser::Serializer;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
#[cfg(feature = "hotpath-meta")]
use std::time::Duration;

#[cfg(feature = "hotpath-meta")]
use crate::FunctionStats;

/// Destination for profiling report output.
#[derive(Default)]
pub enum OutputDestination {
    #[default]
    Stdout,
    File(PathBuf),
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

/// Parses a human-readable duration string back to nanoseconds.
/// Inverse of [`format_duration`].
pub fn parse_duration(s: &str) -> Option<u64> {
    let s = s.trim();
    if let Some(num) = s.strip_suffix(" ns") {
        num.trim().parse::<f64>().ok().map(|v| v.round() as u64)
    } else if let Some(num) = s.strip_suffix(" µs") {
        num.trim()
            .parse::<f64>()
            .ok()
            .map(|v| (v * 1_000.0).round() as u64)
    } else if let Some(num) = s.strip_suffix(" ms") {
        num.trim()
            .parse::<f64>()
            .ok()
            .map(|v| (v * 1_000_000.0).round() as u64)
    } else if let Some(num) = s.strip_suffix(" s") {
        num.trim()
            .parse::<f64>()
            .ok()
            .map(|v| (v * 1_000_000_000.0).round() as u64)
    } else {
        None
    }
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

/// Parses a human-readable byte string back to a byte count.
/// Inverse of [`format_bytes`].
pub fn parse_bytes(s: &str) -> Option<u64> {
    let s = s.trim();
    if let Some(num) = s.strip_suffix(" TB") {
        num.trim()
            .parse::<f64>()
            .ok()
            .map(|v| (v * 1024.0_f64.powi(4)).round() as u64)
    } else if let Some(num) = s.strip_suffix(" GB") {
        num.trim()
            .parse::<f64>()
            .ok()
            .map(|v| (v * 1024.0_f64.powi(3)).round() as u64)
    } else if let Some(num) = s.strip_suffix(" MB") {
        num.trim()
            .parse::<f64>()
            .ok()
            .map(|v| (v * 1024.0_f64.powi(2)).round() as u64)
    } else if let Some(num) = s.strip_suffix(" KB") {
        num.trim()
            .parse::<f64>()
            .ok()
            .map(|v| (v * 1024.0).round() as u64)
    } else if let Some(num) = s.strip_suffix(" B") {
        num.trim().parse::<u64>().ok()
    } else {
        None
    }
}

/// Represents different types of profiling metrics with their values.
#[derive(Debug, Clone)]
pub enum MetricType {
    /// Number of function calls
    CallsCount(u64),
    /// Duration in nanoseconds
    DurationNs(u64),
    /// Bytes allocated, objects allocated
    Alloc(u64, u64),
    /// Percentage as basis points (1% = 100)
    Percentage(u64),
    /// For N/A values (async functions when not supported)
    Unsupported,
}

impl fmt::Display for MetricType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MetricType::CallsCount(count) => write!(f, "{}", count),
            MetricType::DurationNs(ns) => write!(f, "{}", format_duration(*ns)),
            MetricType::Alloc(bytes, _count) => write!(f, "{}", format_bytes(*bytes)),
            MetricType::Percentage(basis_points) => {
                write!(f, "{:.2}%", *basis_points as f64 / 100.0)
            }
            MetricType::Unsupported => write!(f, "N/A*"),
        }
    }
}

/// Profiling mode indicating what type of measurements were collected.
#[derive(Debug, Clone)]
pub enum ProfilingMode {
    /// Time-based profiling (execution duration)
    Timing,
    /// Combined allocation profiling (both bytes and count)
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

#[cfg(all(feature = "hotpath-meta", not(feature = "hotpath-off-meta")))]
static USE_COLORS: std::sync::OnceLock<bool> = std::sync::OnceLock::new();

#[cfg(all(feature = "hotpath-meta", not(feature = "hotpath-off-meta")))]
pub fn set_use_colors(value: bool) {
    let _ = USE_COLORS.set(value);
}

#[cfg(all(feature = "hotpath-meta", not(feature = "hotpath-off-meta")))]
pub fn use_colors() -> bool {
    *USE_COLORS.get().unwrap_or(&false)
}

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
    /// Environment variable `HOTPATH_META_OUTPUT_PATH` takes precedence over programmatic config.
    /// If the path is provided, resolves relative paths against the current working directory.
    /// If no path is provided, returns Stdout.
    pub fn from_path(path: Option<PathBuf>) -> Self {
        if let Ok(env_path) = std::env::var("HOTPATH_META_OUTPUT_PATH") {
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

pub fn floor_char_boundary(s: &str, index: usize) -> usize {
    if index >= s.len() {
        return s.len();
    }
    let mut i = index;
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

pub fn ceil_char_boundary(s: &str, index: usize) -> usize {
    if index >= s.len() {
        return s.len();
    }
    let mut i = index;
    while i < s.len() && !s.is_char_boundary(i) {
        i += 1;
    }
    i
}

pub const MAX_RESULT_LEN: usize = 1536;

#[cfg(all(feature = "hotpath-meta", not(feature = "hotpath-off-meta")))]
struct TruncatingWriter {
    buf: String,
    limit: usize,
    truncated: bool,
}

#[cfg(all(feature = "hotpath-meta", not(feature = "hotpath-off-meta")))]
impl std::fmt::Write for TruncatingWriter {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        if self.truncated {
            return Ok(());
        }

        let remaining = self.limit.saturating_sub(self.buf.len());
        if remaining == 0 {
            if !s.is_empty() {
                self.truncated = true;
            }
            return Ok(());
        }

        let end = floor_char_boundary(s, s.len().min(remaining));

        if end < s.len() {
            self.truncated = true;
        }

        self.buf.push_str(&s[..end]);
        Ok(())
    }
}

#[cfg(all(feature = "hotpath-meta", not(feature = "hotpath-off-meta")))]
pub fn format_debug_truncated(value: &impl std::fmt::Debug) -> String {
    use std::fmt::Write;
    let limit = MAX_RESULT_LEN.saturating_sub(3);
    let mut writer = TruncatingWriter {
        buf: String::with_capacity(64),
        limit,
        truncated: false,
    };
    let _ = write!(writer, "{:?}", value);

    if writer.truncated {
        writer.buf.push_str("...");
    }

    writer.buf
}

pub fn shorten_function_name(function_name: &str) -> String {
    let parts: Vec<&str> = function_name.split("::").collect();
    if parts.len() > 2 {
        parts[parts.len() - 2..].join("::")
    } else {
        function_name.to_string()
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
pub type FunctionsData = Vec<(&'static str, Vec<MetricType>)>;

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

    fn metric_data(&self) -> Vec<(&'static str, Vec<MetricType>)>;

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

    #[cfg(feature = "hotpath-meta")]
    fn new(
        stats: &'a HashMap<u32, FunctionStats>,
        total_elapsed: Duration,
        percentiles: Vec<u8>,
        caller_name: &'static str,
        limit: usize,
    ) -> Self
    where
        Self: Sized;

    fn function_ids(&self) -> HashMap<&'static str, u32> {
        HashMap::new()
    }

    fn total_elapsed(&self) -> u64;

    fn caller_name(&self) -> &str;
}

#[cfg(test)]
mod truncation_tests {
    use super::*;

    #[test]
    fn test_format_debug_truncated() {
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
            let result = format_debug_truncated(&input);
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

#[cfg(test)]
mod parse_tests {
    use super::*;

    #[test]
    fn test_parse_duration_units() {
        assert_eq!(parse_duration("123 ns"), Some(123));
        assert_eq!(parse_duration("0 ns"), Some(0));
        assert_eq!(parse_duration("1.23 µs"), Some(1230));
        assert_eq!(parse_duration("1.23 ms"), Some(1230000));
        assert_eq!(parse_duration("1.23 s"), Some(1230000000));
    }

    #[test]
    fn test_parse_duration_invalid() {
        assert_eq!(parse_duration(""), None);
        assert_eq!(parse_duration("invalid"), None);
        assert_eq!(parse_duration("abc ns"), None);
    }

    #[test]
    fn test_parse_duration_roundtrip() {
        for val in [0, 1, 500, 999, 1000, 50_000, 1_230_000, 1_230_000_000] {
            let formatted = format_duration(val);
            let parsed = parse_duration(&formatted);
            assert_eq!(
                parsed,
                Some(val),
                "round-trip failed for {val}: formatted as '{formatted}'"
            );
        }
    }

    #[test]
    fn test_parse_bytes_units() {
        assert_eq!(parse_bytes("0 B"), Some(0));
        assert_eq!(parse_bytes("123 B"), Some(123));
        assert_eq!(parse_bytes("1.5 KB"), Some(1536));
        assert_eq!(parse_bytes("1.0 MB"), Some(1048576));
        assert_eq!(parse_bytes("1.0 GB"), Some(1073741824));
        assert_eq!(parse_bytes("0.5 TB"), Some(549755813888));
    }

    #[test]
    fn test_parse_bytes_invalid() {
        assert_eq!(parse_bytes(""), None);
        assert_eq!(parse_bytes("invalid"), None);
        assert_eq!(parse_bytes("abc KB"), None);
    }

    #[test]
    fn test_parse_bytes_roundtrip() {
        for val in [0, 100, 1023, 1024, 1536, 1048576, 1073741824] {
            let formatted = format_bytes(val);
            let parsed = parse_bytes(&formatted);
            assert_eq!(
                parsed,
                Some(val),
                "round-trip failed for {val}: formatted as '{formatted}'"
            );
        }
    }
}
