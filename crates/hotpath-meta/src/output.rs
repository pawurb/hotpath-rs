use serde::ser::Serializer;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use std::sync::LazyLock;

const DEFAULT_MAX_LOG_LEN: usize = 1536;
pub static MAX_LOG_LEN: LazyLock<usize> = LazyLock::new(|| {
    std::env::var("HOTPATH_META_MAX_LOG_LEN")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_MAX_LOG_LEN)
});

const DEFAULT_FUNCTIONS_NAME_DEPTH: usize = 2;
pub static FUNCTIONS_NAME_DEPTH: LazyLock<usize> = LazyLock::new(|| {
    std::env::var("HOTPATH_META_FUNCTIONS_NAME_DEPTH")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_FUNCTIONS_NAME_DEPTH)
});

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

/// Formats a percentile value for use as a map key (e.g., `"p95"`, `"p99.9"`).
pub fn format_percentile_key(p: f64) -> String {
    if p.fract() == 0.0 {
        format!("p{}", p as u64)
    } else {
        format!("p{}", p)
    }
}

/// Formats a percentile value for display as a column header (e.g., `"P95"`, `"P99.9"`).
pub fn format_percentile_header(p: f64) -> String {
    if p.fract() == 0.0 {
        format!("P{}", p as u64)
    } else {
        format!("P{}", p)
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

/// Formats an optional per-second rate to one decimal place, or `-` when absent.
pub fn format_rate(rate: Option<f64>) -> String {
    rate.map_or_else(|| "-".to_string(), |v| format!("{v:.1}"))
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

/// Formats an allocation count as a string.
pub fn format_count(count: u64) -> String {
    count.to_string()
}

/// Parses a count string back to a u64.
/// Inverse of [`format_count`].
pub fn parse_count(s: &str) -> Option<u64> {
    s.trim().parse::<u64>().ok()
}

/// Profiling mode indicating what type of measurements were collected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProfilingMode {
    /// Time-based profiling (execution duration)
    Timing,
    /// Allocation profiling with bytes as primary metric
    AllocBytes,
    /// Allocation profiling with count as primary metric
    AllocCount,
}

impl fmt::Display for ProfilingMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProfilingMode::Timing => write!(f, "timing"),
            ProfilingMode::AllocBytes => write!(f, "alloc-bytes"),
            ProfilingMode::AllocCount => write!(f, "alloc-count"),
        }
    }
}

#[cfg(feature = "hotpath-meta")]
static USE_COLORS: std::sync::OnceLock<bool> = std::sync::OnceLock::new();

#[cfg(feature = "hotpath-meta")]
pub(crate) fn set_use_colors(value: bool) {
    let _ = USE_COLORS.set(value);
}

#[cfg(feature = "hotpath-meta")]
pub(crate) fn use_colors() -> bool {
    *USE_COLORS.get().unwrap_or(&false)
}

#[cfg(feature = "hotpath-cpu-meta")]
pub(crate) fn cyan(text: &str) -> String {
    if use_colors() {
        format!("\x1b[1;36m{text}\x1b[0m")
    } else {
        text.to_string()
    }
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
pub(crate) fn resolve_output_path(path: impl AsRef<std::path::Path>) -> PathBuf {
    let path = path.as_ref();
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map(|cwd| cwd.join(path))
            .unwrap_or_else(|_| path.to_path_buf())
    }
}

impl Serialize for ProfilingMode {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            ProfilingMode::Timing => serializer.serialize_str("timing"),
            ProfilingMode::AllocBytes => serializer.serialize_str("alloc-bytes"),
            ProfilingMode::AllocCount => serializer.serialize_str("alloc-count"),
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
            "alloc-bytes" => Ok(ProfilingMode::AllocBytes),
            "alloc-count" => Ok(ProfilingMode::AllocCount),
            _ => Err(serde::de::Error::unknown_variant(
                &s,
                &["timing", "alloc-bytes", "alloc-count"],
            )),
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

#[cfg(feature = "hotpath-meta")]
struct TruncatingWriter {
    buf: String,
    limit: usize,
    truncated: bool,
}

#[cfg(feature = "hotpath-meta")]
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

#[cfg(feature = "hotpath-meta")]
pub fn format_debug_truncated(value: &impl std::fmt::Debug) -> String {
    let _suspend = crate::lib_on::SuspendAllocTracking::new();
    use std::fmt::Write;
    let limit = MAX_LOG_LEN.saturating_sub(3);
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
    let depth = *FUNCTIONS_NAME_DEPTH;
    if depth == 0 {
        return function_name.to_string();
    }
    let parts: Vec<&str> = function_name.split("::").collect();
    if parts.len() > depth {
        parts[parts.len() - depth..].join("::")
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
#[allow(dead_code)]
pub(crate) struct FunctionLog {
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
#[allow(dead_code)]
pub(crate) struct FunctionLogsList {
    pub function_name: String,
    pub logs: Vec<FunctionLog>,
    /// Total number of times this function was invoked (used to calculate invocation numbers)
    pub count: usize,
}

#[cfg(test)]
mod truncation_tests {
    use super::*;

    #[test]
    fn test_format_debug_truncated() {
        let truncate_point = MAX_LOG_LEN.saturating_sub(3);

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
            if input.len() > *MAX_LOG_LEN {
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

    #[test]
    fn test_format_count() {
        assert_eq!(format_count(0), "0");
        assert_eq!(format_count(999), "999");
        assert_eq!(format_count(1_000), "1000");
        assert_eq!(format_count(1_000_000), "1000000");
    }

    #[test]
    fn test_parse_count_roundtrip() {
        for val in [0, 1, 500, 999, 1_000, 1_500, 50_000, 1_000_000] {
            let formatted = format_count(val);
            let parsed = parse_count(&formatted);
            assert_eq!(
                parsed,
                Some(val),
                "round-trip failed for {val}: formatted as '{formatted}'"
            );
        }
    }
}
