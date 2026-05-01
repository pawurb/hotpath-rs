use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::sync::{LazyLock, Mutex, OnceLock};

use prettytable::{color, Attr, Cell, Row, Table};

use crate::json::{JsonFunctionCpuEntry, JsonFunctionsCpuList};
use crate::output::{format_duration, shorten_function_name};

const DEFAULT_CPU_SAMPLE_RATE_HZ: u32 = 1000;

pub(crate) static CPU_SAMPLE_RATE_HZ: LazyLock<u32> = LazyLock::new(|| {
    std::env::var("HOTPATH_CPU_SAMPLE_RATE_HZ")
        .ok()
        .and_then(|s| s.parse::<u32>().ok())
        .filter(|&v| v > 0)
        .unwrap_or(DEFAULT_CPU_SAMPLE_RATE_HZ)
});

pub(crate) static CPU_INCLUSIVE: LazyLock<bool> =
    LazyLock::new(|| crate::shared::env_flag("HOTPATH_CPU_INCLUSIVE"));

pub(crate) static PPROF_GUARD: OnceLock<Mutex<Option<pprof::ProfilerGuard<'static>>>> =
    OnceLock::new();

static IP_ATTR_CACHE: LazyLock<Mutex<HashMap<usize, Option<&'static str>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

#[cfg_attr(feature = "hotpath-meta", hotpath_meta::measure(log = true))]
pub(crate) fn install_pprof_guard(guard: pprof::ProfilerGuard<'static>) {
    let _ = PPROF_GUARD.set(Mutex::new(Some(guard)));
}

#[cfg_attr(feature = "hotpath-meta", hotpath_meta::measure)]
pub(crate) fn take_pprof_guard() -> Option<pprof::ProfilerGuard<'static>> {
    PPROF_GUARD.get()?.lock().ok()?.take()
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
    pprof_guard: &pprof::ProfilerGuard<'static>,
    caller_name: &'static str,
) -> Option<CpuReport> {
    let unresolved = match build_unresolved_report(pprof_guard) {
        Some(report) => report,
        None => return None,
    };

    let names_and_ids = crate::functions::get_instrumented_names_and_ids()?;
    let mut eligible: HashMap<&'static str, u32> = names_and_ids;
    let eligible_names: HashSet<&'static str> = eligible.keys().copied().collect();

    let (total_samples, attributed) = attribute_unresolved(&unresolved, &eligible_names);
    let attributed_samples: u64 = attributed.values().sum();

    let mut stats: Vec<CpuFunctionStats> = attributed
        .into_iter()
        .map(|(name, samples)| CpuFunctionStats {
            id: eligible.remove(name).unwrap_or(0),
            name,
            samples,
        })
        .collect();

    stats.sort_by(|a, b| b.samples.cmp(&a.samples).then_with(|| a.name.cmp(b.name)));

    Some(CpuReport {
        total_samples,
        attributed_samples,
        sample_rate_hz: *CPU_SAMPLE_RATE_HZ,
        caller_name,
        stats,
    })
}

#[cfg_attr(feature = "hotpath-meta", hotpath_meta::measure)]
fn build_unresolved_report(
    pprof_guard: &pprof::ProfilerGuard<'static>,
) -> Option<pprof::UnresolvedReport> {
    match pprof_guard.report().build_unresolved() {
        Ok(report) => Some(report),
        Err(e) => {
            eprintln!("[hotpath - cpu] failed to build pprof report: {}", e);
            None
        }
    }
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

fn resolve_ip<F: pprof::backtrace::Frame>(
    frame: &F,
    eligible_names: &HashSet<&'static str>,
    cache: &mut HashMap<usize, Option<&'static str>>,
) -> Option<&'static str> {
    let ip = frame.ip();
    if let Some(hit) = cache.get(&ip) {
        return *hit;
    }
    let mut found: Option<&'static str> = None;
    frame.resolve_symbol(|sym| {
        if found.is_some() {
            return;
        }
        let raw = match pprof::backtrace::Symbol::name(sym) {
            Some(b) => b,
            None => return,
        };
        let tmp = pprof::Symbol {
            name: Some(raw),
            addr: None,
            lineno: None,
            filename: None,
        };
        if let Some(name) = eligible_names.get(tmp.name().as_str()) {
            found = Some(*name);
        }
    });
    cache.insert(ip, found);
    found
}

#[cfg_attr(feature = "hotpath-meta", hotpath_meta::measure(log = true))]
fn attribute_unresolved(
    report: &pprof::UnresolvedReport,
    eligible_names: &HashSet<&'static str>,
) -> (u64, HashMap<&'static str, u64>) {
    let mut total_samples: u64 = 0;
    let mut attributed: HashMap<&'static str, u64> = HashMap::new();
    let mut cache = match IP_ATTR_CACHE.lock() {
        Ok(c) => c,
        Err(_) => return (0, attributed),
    };
    let inclusive = *CPU_INCLUSIVE;

    for (frames, count) in &report.data {
        let count = match u64::try_from(*count) {
            Ok(c) if c > 0 => c,
            _ => continue,
        };
        total_samples += count;

        if inclusive {
            let mut seen: HashSet<&'static str> = HashSet::new();
            for frame in frames.frames.iter() {
                if let Some(name) = resolve_ip(frame, eligible_names, &mut cache) {
                    seen.insert(name);
                }
            }
            for name in seen {
                *attributed.entry(name).or_default() += count;
            }
        } else {
            for frame in frames.frames.iter() {
                if let Some(name) = resolve_ip(frame, eligible_names, &mut cache) {
                    *attributed.entry(name).or_default() += count;
                    break;
                }
            }
        }
    }

    (total_samples, attributed)
}
