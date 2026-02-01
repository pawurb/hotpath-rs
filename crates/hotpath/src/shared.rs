use std::fmt;
use std::path::PathBuf;
use std::str::FromStr;

/// Output format for profiling reports.
///
/// This enum specifies how profiling results should be displayed when the program exits.
///
/// # Variants
///
/// * `Table` - Human-readable table format (default)
/// * `Json` - JSON format
/// * `JsonPretty` - Pretty-printed JSON format
/// * `None` - Suppress all profiling output (metrics server and MCP server still function)
///
/// # Parsing
///
/// Can be parsed from strings via `HOTPATH_OUTPUT_FORMAT` environment variable:
/// - `"table"` → `Format::Table`
/// - `"json"` → `Format::Json`
/// - `"json-pretty"` → `Format::JsonPretty`
/// - `"none"` → `Format::None`
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Format {
    #[default]
    Table,
    Json,
    JsonPretty,
    None,
}

impl FromStr for Format {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "table" => Ok(Format::Table),
            "json" => Ok(Format::Json),
            "json-pretty" | "jsonpretty" => Ok(Format::JsonPretty),
            "none" => Ok(Format::None),
            _ => Err(format!(
                "unknown format '{}', expected: table, json, json-pretty, none",
                s
            )),
        }
    }
}

impl Format {
    /// Returns the format from `HOTPATH_OUTPUT_FORMAT` env var, or default if not set.
    /// Panics if the env var contains an invalid value.
    pub fn from_env() -> Self {
        match std::env::var("HOTPATH_OUTPUT_FORMAT") {
            Ok(v) => v
                .parse()
                .unwrap_or_else(|e| panic!("HOTPATH_OUTPUT_FORMAT: {}", e)),
            Err(_) => Format::default(),
        }
    }
}

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
/// * `Alloc(u64, u64)` - Bytes allocated and allocation count (formatted with KB/MB/GB units)
/// * `Percentage(u64)` - Percentage as basis points (1% = 100, formatted as percentage)
/// * `Unsupported` - For N/A values (e.g., async functions when allocation profiling not supported)
///
/// # Examples
///
/// ```rust
/// use hotpath::MetricType;
///
/// let duration = MetricType::DurationNs(1_500_000); // 1.5ms
/// let memory = MetricType::Alloc(2048, 1); // 2KB, 1 allocation
/// let percent = MetricType::Percentage(9500); // 95.00%
///
/// println!("{}", duration); // Displays: "1.50ms"
/// println!("{}", memory);   // Displays: "2.0 KB"
/// println!("{}", percent);  // Displays: "95.00%"
/// ```
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
///
/// This enum identifies which profiling feature was active when measurements
/// were collected. It's included in JSON output to help interpret the metrics.
#[allow(dead_code)]
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

pub trait IntoF64 {
    fn into_f64(self) -> f64;
}

impl IntoF64 for f64 {
    fn into_f64(self) -> f64 {
        self
    }
}

impl IntoF64 for f32 {
    fn into_f64(self) -> f64 {
        self as f64
    }
}

impl IntoF64 for i8 {
    fn into_f64(self) -> f64 {
        self as f64
    }
}

impl IntoF64 for i16 {
    fn into_f64(self) -> f64 {
        self as f64
    }
}

impl IntoF64 for i32 {
    fn into_f64(self) -> f64 {
        self as f64
    }
}

impl IntoF64 for i64 {
    fn into_f64(self) -> f64 {
        self as f64
    }
}

impl IntoF64 for u8 {
    fn into_f64(self) -> f64 {
        self as f64
    }
}

impl IntoF64 for u16 {
    fn into_f64(self) -> f64 {
        self as f64
    }
}

impl IntoF64 for u32 {
    fn into_f64(self) -> f64 {
        self as f64
    }
}

impl IntoF64 for u64 {
    fn into_f64(self) -> f64 {
        self as f64
    }
}

impl IntoF64 for isize {
    fn into_f64(self) -> f64 {
        self as f64
    }
}

impl IntoF64 for usize {
    fn into_f64(self) -> f64 {
        self as f64
    }
}
