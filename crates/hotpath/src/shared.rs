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
