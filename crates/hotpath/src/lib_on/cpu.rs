use std::collections::HashMap;
use std::io::Write;
use std::sync::{LazyLock, Mutex, OnceLock, RwLock};

use prettytable::{color, Attr, Cell, Row, Table};

use crate::json::{JsonFunctionCpuEntry, JsonFunctionsCpuList};
use crate::output::{format_duration, shorten_function_name};

const DEFAULT_CPU_SAMPLE_RATE_HZ: u32 = 1000;

pub(crate) const ENV_PROFILE_PATH: &str = "HOTPATH_CPU_PROFILE_PATH";

static SYMBOL_MAP: LazyLock<RwLock<HashMap<&'static str, &'static str>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

pub(crate) fn register_symbol(display: &'static str, symbol: &'static str) {
    if let Ok(mut map) = SYMBOL_MAP.write() {
        map.entry(display).or_insert(symbol);
    }
}

pub(crate) fn get_symbol_map() -> HashMap<&'static str, &'static str> {
    SYMBOL_MAP.read().map(|m| m.clone()).unwrap_or_default()
}

pub(crate) static CPU_SAMPLE_RATE_HZ: LazyLock<u32> = LazyLock::new(|| {
    std::env::var("HOTPATH_CPU_SAMPLE_RATE_HZ")
        .ok()
        .and_then(|s| s.parse::<u32>().ok())
        .filter(|&v| v > 0)
        .unwrap_or(DEFAULT_CPU_SAMPLE_RATE_HZ)
});

pub(crate) static CPU_INCLUSIVE: LazyLock<bool> =
    LazyLock::new(|| crate::shared::env_flag("HOTPATH_CPU_INCLUSIVE"));

pub(crate) struct CpuSamplerHandle;

pub(crate) static CPU_SAMPLER: OnceLock<Mutex<Option<CpuSamplerHandle>>> = OnceLock::new();

#[cfg_attr(feature = "hotpath-meta", hotpath_meta::measure)]
pub(crate) fn install_cpu_sampler() {
    let _ = CPU_SAMPLER.set(Mutex::new(Some(CpuSamplerHandle)));
}

#[cfg_attr(feature = "hotpath-meta", hotpath_meta::measure)]
pub(crate) fn take_cpu_sampler() -> Option<CpuSamplerHandle> {
    CPU_SAMPLER.get()?.lock().ok()?.take()
}

#[cfg_attr(feature = "hotpath-meta", hotpath_meta::measure(log = true))]
pub(crate) fn build_cpu_report_live() -> Option<JsonFunctionsCpuList> {
    None
}

#[derive(Debug, Clone)]
pub(crate) struct CpuFunctionStats {
    pub(crate) name: &'static str,
    pub(crate) id: u32,
    pub(crate) samples: u64,
}

#[derive(Debug, Clone)]
pub(crate) struct CpuReport {
    pub(crate) total_samples: u64,
    pub(crate) attributed_samples: u64,
    pub(crate) sample_rate_hz: u32,
    pub(crate) caller_name: &'static str,
    pub(crate) stats: Vec<CpuFunctionStats>,
}

#[cfg_attr(feature = "hotpath-meta", hotpath_meta::measure(log = true))]
pub(crate) fn build_cpu_report(
    _sampler: &CpuSamplerHandle,
    caller_name: &'static str,
) -> Option<CpuReport> {
    crate::lib_on::cpu_samply::build_cpu_report_from_samply(caller_name)
}

#[cfg_attr(feature = "hotpath-meta", hotpath_meta::measure(log = true))]
fn format_percent(numer: u64, denom: u64) -> String {
    if denom == 0 {
        "0.00%".to_string()
    } else {
        format!("{:.2}%", (numer as f64 / denom as f64) * 100.0)
    }
}

#[cfg_attr(feature = "hotpath-meta", hotpath_meta::measure(log = true))]
pub(crate) fn build_cpu_json(
    report: &CpuReport,
    total_elapsed: std::time::Duration,
    current_elapsed_ns: u64,
    limit: usize,
) -> JsonFunctionsCpuList {
    let (wrapper_stats, inner_stats): (Vec<_>, Vec<_>) = report
        .stats
        .iter()
        .partition(|s| s.name == report.caller_name);

    let total_inner = inner_stats.len();
    let displayed_inner = if limit > 0 && limit < total_inner {
        limit
    } else {
        total_inner
    };

    let to_entry = |s: &CpuFunctionStats| JsonFunctionCpuEntry {
        id: s.id,
        name: s.name.to_string(),
        samples: s.samples,
        percent: format_percent(s.samples, report.total_samples),
    };

    let mut entries: Vec<JsonFunctionCpuEntry> =
        wrapper_stats.iter().map(|s| to_entry(s)).collect();
    entries.extend(
        inner_stats
            .iter()
            .take(displayed_inner)
            .map(|s| to_entry(s)),
    );

    let total_count = total_inner + wrapper_stats.len();
    let displayed_count = displayed_inner + wrapper_stats.len();

    let description = if *CPU_INCLUSIVE {
        "CPU sampling attribution per function (inclusive).".to_string()
    } else {
        "CPU sampling attribution per function (exclusive).".to_string()
    };

    JsonFunctionsCpuList {
        time_elapsed: format_duration(total_elapsed.as_nanos() as u64),
        total_elapsed_ns: current_elapsed_ns,
        total_samples: report.total_samples,
        attributed_samples: report.attributed_samples,
        sample_rate_hz: report.sample_rate_hz,
        description,
        caller_name: report.caller_name.to_string(),
        data: entries,
        displayed_count,
        total_count,
    }
}

#[cfg_attr(feature = "hotpath-meta", hotpath_meta::measure(log = true))]
fn styled_header(text: &str) -> Cell {
    if crate::output::use_colors() {
        Cell::new(text)
            .with_style(Attr::Bold)
            .with_style(Attr::ForegroundColor(color::CYAN))
    } else {
        Cell::new(text).with_style(Attr::Bold)
    }
}

#[cfg_attr(feature = "hotpath-meta", hotpath_meta::measure(log = true))]
fn print_table<W: Write>(table: &Table, writer: &mut W) {
    if crate::output::use_colors() {
        let _ = table.print_tty(false);
    } else {
        let _ = table.print(writer);
    }
}

#[cfg_attr(feature = "hotpath-meta", hotpath_meta::measure(log = true))]
pub(crate) fn report_functions_cpu_table<W: Write>(writer: &mut W, list: &JsonFunctionsCpuList) {
    if list.data.is_empty() {
        return;
    }

    let mut table = Table::new();
    table.add_row(Row::new(vec![
        styled_header("Function"),
        styled_header("Samples"),
        styled_header("% Total"),
    ]));

    for entry in &list.data {
        let short_name = shorten_function_name(&entry.name);
        table.add_row(Row::new(vec![
            Cell::new(&short_name),
            Cell::new(&entry.samples.to_string()),
            Cell::new(&entry.percent),
        ]));
    }

    let mut info = format!("{} total samples", list.total_samples);
    if list.displayed_count < list.total_count {
        info.push_str(&format!(", {}/{}", list.displayed_count, list.total_count));
    }
    let _ = writeln!(writer, "cpu - {} ({})", list.description, info);
    print_table(&table, writer);
    let _ = writeln!(writer);
}
