use std::str::FromStr;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Section {
    FunctionsTiming,
    FunctionsAlloc,
    FunctionsCpu,
    Channels,
    Streams,
    Futures,
    RwLocks,
    Mutexes,
    Threads,
    Debug,
}

impl Section {
    pub fn all() -> Vec<Section> {
        vec![
            Section::FunctionsTiming,
            Section::FunctionsAlloc,
            Section::FunctionsCpu,
            Section::Channels,
            Section::Streams,
            Section::Futures,
            Section::RwLocks,
            Section::Mutexes,
            Section::Threads,
            Section::Debug,
        ]
    }

    pub fn short_name(&self) -> &'static str {
        match self {
            Section::FunctionsTiming => "timing",
            Section::FunctionsAlloc => "alloc",
            Section::FunctionsCpu => "cpu",
            Section::Channels => "channels",
            Section::Streams => "streams",
            Section::Futures => "futures",
            Section::RwLocks => "rw_locks",
            Section::Mutexes => "mutexes",
            Section::Threads => "threads",
            Section::Debug => "debug",
        }
    }

    pub fn from_name(s: &str) -> Option<Section> {
        match s.trim() {
            "functions-timing" => Some(Section::FunctionsTiming),
            "functions-alloc" => Some(Section::FunctionsAlloc),
            "functions-cpu" => Some(Section::FunctionsCpu),
            "channels" => Some(Section::Channels),
            "streams" => Some(Section::Streams),
            "futures" => Some(Section::Futures),
            "rw_locks" => Some(Section::RwLocks),
            "mutexes" => Some(Section::Mutexes),
            "threads" => Some(Section::Threads),
            "debug" => Some(Section::Debug),
            _ => None,
        }
    }

    pub fn from_env() -> Option<Vec<Section>> {
        std::env::var("HOTPATH_META_REPORT").ok().map(|val| {
            let mut sections = Vec::new();
            for part in val.split(',') {
                match part.trim() {
                    "all" => return Section::all(),
                    other => {
                        if let Some(s) = Section::from_name(other) {
                            if !sections.contains(&s) {
                                sections.push(s);
                            }
                        } else {
                            eprintln!("[hotpath-meta] Unknown report section: '{}'", other);
                        }
                    }
                }
            }
            sections
        })
    }
}

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
/// Can be parsed from strings via `HOTPATH_META_OUTPUT_FORMAT` environment variable:
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
    /// Returns the format from `HOTPATH_META_OUTPUT_FORMAT` env var, or default if not set.
    /// Panics if the env var contains an invalid value.
    pub fn from_env() -> Self {
        match std::env::var("HOTPATH_META_OUTPUT_FORMAT") {
            Ok(v) => v
                .parse()
                .unwrap_or_else(|e| panic!("HOTPATH_META_OUTPUT_FORMAT: {}", e)),
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

#[doc(hidden)]
pub fn env_flag(name: &str) -> bool {
    std::env::var(name)
        .map(|v| v.eq_ignore_ascii_case("true") || v == "1")
        .unwrap_or(false)
}

#[cfg(feature = "hotpath-meta")]
pub(crate) fn resolve_timeout_duration(
    default_duration: std::time::Duration,
    env_var: &str,
) -> Option<std::time::Duration> {
    let effective_duration = std::env::var(env_var)
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .map(std::time::Duration::from_millis)
        .unwrap_or(default_duration);

    if effective_duration.is_zero() {
        None
    } else {
        Some(effective_duration)
    }
}
