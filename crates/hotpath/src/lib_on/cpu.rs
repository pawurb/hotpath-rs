use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::sync::{LazyLock, Mutex, OnceLock};
use std::time::{Duration, Instant};

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

pub(crate) static PPROF_GUARD: OnceLock<Mutex<Option<pprof::ProfilerGuard<'static>>>> =
    OnceLock::new();

const LIVE_REPORT_TTL: Duration = Duration::from_secs(2);

static LIVE_REPORT_CACHE: LazyLock<Mutex<Option<(Instant, JsonFunctionsCpuList)>>> =
    LazyLock::new(|| Mutex::new(None));

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
    if let Ok(cache) = LIVE_REPORT_CACHE.lock() {
        if let Some((stamped_at, ref cached)) = *cache {
            if stamped_at.elapsed() < LIVE_REPORT_TTL {
                return Some(cached.clone());
            }
        }
    }

    let guard_slot = PPROF_GUARD.get()?;
    let guard = guard_slot.lock().ok()?;
    let pprof_guard = guard.as_ref()?;

    let caller_name = crate::lib_on::functions::FUNCTIONS_STATE
        .get()?
        .read()
        .ok()?
        .caller_name;

    let report = build_cpu_report(pprof_guard, caller_name)?;
    let elapsed_ns = crate::lib_on::current_elapsed_ns();
    let json = build_cpu_json(&report, Duration::from_nanos(elapsed_ns), elapsed_ns, 0);

    if let Ok(mut cache) = LIVE_REPORT_CACHE.lock() {
        *cache = Some((Instant::now(), json.clone()));
    }

    Some(json)
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
    let report = match pprof_guard.report().build() {
        Ok(report) => report,
        Err(e) => {
            eprintln!("[hotpath - cpu] failed to build pprof report: {}", e);
            return None;
        }
    };

    let names_and_ids = crate::functions::get_instrumented_names_and_ids()?;

    let total_samples: u64 = report
        .data
        .values()
        .filter_map(|v| u64::try_from(*v).ok())
        .sum();

    let caller_id = names_and_ids.get(caller_name).copied().unwrap_or(0);
    let mut eligible: HashMap<&'static str, u32> = names_and_ids
        .into_iter()
        .filter(|(name, _)| *name != caller_name)
        .collect();

    let eligible_names: HashSet<&'static str> = eligible.keys().copied().collect();
    let attributed = attribute_exclusive_traces(&report, &eligible_names);
    let attributed_samples: u64 = attributed.values().sum();

    let mut stats: Vec<CpuFunctionStats> = attributed
        .into_iter()
        .map(|(name, samples)| CpuFunctionStats {
            id: eligible.remove(name).unwrap_or(0),
            name,
            samples,
        })
        .collect();

    stats.push(CpuFunctionStats {
        id: caller_id,
        name: caller_name,
        samples: total_samples,
    });

    stats.sort_by(|a, b| b.samples.cmp(&a.samples).then_with(|| a.name.cmp(b.name)));

    Some(CpuReport {
        total_samples,
        attributed_samples,
        sample_rate_hz: *CPU_SAMPLE_RATE_HZ,
        caller_name,
        stats,
    })
}

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

    JsonFunctionsCpuList {
        time_elapsed: format_duration(total_elapsed.as_nanos() as u64),
        total_elapsed_ns: current_elapsed_ns,
        total_samples: report.total_samples,
        attributed_samples: report.attributed_samples,
        sample_rate_hz: report.sample_rate_hz,
        description: "CPU sampling attribution per function (exclusive).".to_string(),
        caller_name: report.caller_name.to_string(),
        data: entries,
        displayed_count,
        total_count,
    }
}

fn styled_header(text: &str) -> Cell {
    if crate::output::use_colors() {
        Cell::new(text)
            .with_style(Attr::Bold)
            .with_style(Attr::ForegroundColor(color::CYAN))
    } else {
        Cell::new(text).with_style(Attr::Bold)
    }
}

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

#[cfg_attr(feature = "hotpath-meta", hotpath_meta::measure(log = true))]
fn attribute_exclusive_traces(
    report: &pprof::Report,
    eligible_names: &HashSet<&'static str>,
) -> HashMap<&'static str, u64> {
    let mut attributed = HashMap::<&'static str, u64>::new();

    for (stack, samples) in &report.data {
        let samples = match u64::try_from(*samples) {
            Ok(samples) if samples > 0 => samples,
            _ => continue,
        };

        let mut owner: Option<&'static str> = None;
        for frame in &stack.frames {
            for sym in frame {
                let symbol = format!("{sym}");
                if let Some(name) = eligible_names.get(symbol.as_str()) {
                    owner = Some(*name);
                    break;
                }
            }

            if owner.is_some() {
                break;
            }
        }

        if let Some(owner) = owner {
            *attributed.entry(owner).or_default() += samples;
        }
    }

    attributed
}
