use hotpath::json::{JsonFunctionEntry, JsonFunctionsList, JsonReport};
use hotpath::{format_bytes, parse_bytes, parse_duration};
use std::fmt;
use std::time::Duration;

#[derive(Debug, Clone)]
pub enum MetricDiff {
    CallsCount(u64, u64), // (before, after)
    DurationNs(u64, u64), // (before, after) - Duration in nanoseconds
    Alloc(u64, u64),      // (before, after) - Bytes allocated
    Percentage(u64, u64), // (before, after)
}

impl fmt::Display for MetricDiff {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.format_with_emoji(None))
    }
}

impl MetricDiff {
    pub fn format_with_emoji(&self, emoji_threshold: Option<u32>) -> String {
        match self {
            MetricDiff::CallsCount(before, after) => {
                let diff_percent = calculate_percentage_diff(*before, *after);
                let emoji = get_emoji_for_diff(diff_percent, emoji_threshold);
                format!("{} → {} ({:+.1}%){}", before, after, diff_percent, emoji)
            }
            MetricDiff::DurationNs(before, after) => {
                let diff_percent = calculate_percentage_diff(*before, *after);
                let before_duration = Duration::from_nanos(*before);
                let after_duration = Duration::from_nanos(*after);
                let emoji = get_emoji_for_diff(diff_percent, emoji_threshold);
                format!(
                    "{:.2?} → {:.2?} ({:+.1}%){}",
                    before_duration, after_duration, diff_percent, emoji
                )
            }
            MetricDiff::Alloc(before, after) => {
                let diff_percent = calculate_percentage_diff(*before, *after);
                let emoji = get_emoji_for_diff(diff_percent, emoji_threshold);
                format!(
                    "{} → {} ({:+.1}%){}",
                    format_bytes(*before),
                    format_bytes(*after),
                    diff_percent,
                    emoji
                )
            }
            MetricDiff::Percentage(before, after) => {
                let diff_percent = calculate_percentage_diff(*before, *after);
                let before_percent = *before as f64 / 100.0;
                let after_percent = *after as f64 / 100.0;
                let emoji = get_emoji_for_diff(diff_percent, emoji_threshold);
                format!(
                    "{:.2}% → {:.2}% ({:+.1}%){}",
                    before_percent, after_percent, diff_percent, emoji
                )
            }
        }
    }
}

fn get_emoji_for_diff(diff_percent: f64, threshold: Option<u32>) -> &'static str {
    if let Some(threshold_val) = threshold {
        let threshold = threshold_val as f64;
        if diff_percent > threshold {
            " ⚠️ "
        } else if diff_percent < -threshold {
            " 🚀 "
        } else {
            "   "
        }
    } else {
        ""
    }
}

#[derive(Debug, Clone)]
pub struct MetricsComparison {
    pub profiling_mode: hotpath::ProfilingMode,
    pub description: String,
    pub percentiles: Vec<u8>,
    pub before_elapsed: String,
    pub after_elapsed: String,
    pub total_elapsed_diff: MetricDiff,
    pub function_diffs: Vec<FunctionMetricsDiff>,
}

#[derive(Debug, Clone)]
pub struct FunctionMetricsDiff {
    pub function_name: String,
    pub metrics: Vec<MetricDiff>,
    pub is_removed: bool,
    pub is_new: bool,
}

#[derive(Debug, Clone)]
pub struct JsonReportDiff {
    pub functions_timing: Option<MetricsComparison>,
    pub functions_alloc: Option<MetricsComparison>,
}

pub fn compare_reports(before: &JsonReport, after: &JsonReport) -> JsonReportDiff {
    let functions_timing = match (&before.functions_timing, &after.functions_timing) {
        (Some(b), Some(a)) => Some(compare_metrics(b, a)),
        _ => None,
    };

    let functions_alloc = match (&before.functions_alloc, &after.functions_alloc) {
        (Some(b), Some(a)) => Some(compare_metrics(b, a)),
        _ => None,
    };

    JsonReportDiff {
        functions_timing,
        functions_alloc,
    }
}

fn calculate_percentage_diff(before: u64, after: u64) -> f64 {
    if before == 0 {
        if after == 0 {
            0.0
        } else {
            100.0
        }
    } else {
        ((after as f64 - before as f64) / before as f64) * 100.0
    }
}

fn find_function<'a>(data: &'a [JsonFunctionEntry], name: &str) -> Option<&'a JsonFunctionEntry> {
    data.iter().find(|f| f.name == name)
}

fn parse_value(s: &str, is_alloc: bool) -> Option<u64> {
    if is_alloc {
        parse_bytes(s)
    } else {
        parse_duration(s)
    }
}

fn parse_percent(s: &str) -> Option<u64> {
    let s = s.trim().trim_end_matches('%').trim();
    let pct: f64 = s.parse().ok()?;
    Some((pct * 100.0).round() as u64)
}

#[derive(Debug, Clone, Copy)]
enum MetricKind {
    Calls,
    Duration,
    Alloc,
    Percentage,
}

fn build_metrics_from_function(
    func: &JsonFunctionEntry,
    percentiles: &[u8],
    is_alloc: bool,
) -> Vec<(MetricKind, u64)> {
    let mut metrics = Vec::new();
    let kind = if is_alloc {
        MetricKind::Alloc
    } else {
        MetricKind::Duration
    };

    metrics.push((MetricKind::Calls, func.calls));

    if let Some(val) = parse_value(&func.avg, is_alloc) {
        metrics.push((kind, val));
    }

    for p in percentiles {
        let key = format!("p{}", p);
        if let Some(formatted) = func.percentiles.get(&key) {
            if let Some(val) = parse_value(formatted, is_alloc) {
                metrics.push((kind, val));
            }
        }
    }

    if let Some(val) = parse_value(&func.total, is_alloc) {
        metrics.push((kind, val));
    }

    if let Some(bp) = parse_percent(&func.percent_total) {
        metrics.push((MetricKind::Percentage, bp));
    }

    metrics
}

pub fn compare_metrics(
    before_metrics: &JsonFunctionsList,
    after_metrics: &JsonFunctionsList,
) -> MetricsComparison {
    use hotpath::ProfilingMode;

    let is_alloc = matches!(before_metrics.hotpath_profiling_mode, ProfilingMode::Alloc);

    let before_elapsed = parse_duration(&before_metrics.time_elapsed).unwrap_or(0);
    let after_elapsed = parse_duration(&after_metrics.time_elapsed).unwrap_or(0);
    let total_elapsed_diff = MetricDiff::DurationNs(before_elapsed, after_elapsed);

    let mut function_diffs = Vec::new();
    let mut new_functions = Vec::new();

    for after_func in &after_metrics.data {
        if let Some(before_func) = find_function(&before_metrics.data, &after_func.name) {
            let before_vals =
                build_metrics_from_function(before_func, &before_metrics.percentiles, is_alloc);
            let after_vals =
                build_metrics_from_function(after_func, &after_metrics.percentiles, is_alloc);

            let mut metrics = Vec::new();
            for ((kind, before_val), (_, after_val)) in before_vals.iter().zip(after_vals.iter()) {
                let diff = match kind {
                    MetricKind::Calls => MetricDiff::CallsCount(*before_val, *after_val),
                    MetricKind::Duration => MetricDiff::DurationNs(*before_val, *after_val),
                    MetricKind::Alloc => MetricDiff::Alloc(*before_val, *after_val),
                    MetricKind::Percentage => MetricDiff::Percentage(*before_val, *after_val),
                };
                metrics.push(diff);
            }

            function_diffs.push(FunctionMetricsDiff {
                function_name: after_func.name.clone(),
                metrics,
                is_removed: false,
                is_new: false,
            });
        } else {
            let after_vals =
                build_metrics_from_function(after_func, &after_metrics.percentiles, is_alloc);

            let metrics = after_vals
                .iter()
                .map(|(kind, after_val)| match kind {
                    MetricKind::Calls => MetricDiff::CallsCount(0, *after_val),
                    MetricKind::Duration => MetricDiff::DurationNs(0, *after_val),
                    MetricKind::Alloc => MetricDiff::Alloc(0, *after_val),
                    MetricKind::Percentage => MetricDiff::Percentage(0, *after_val),
                })
                .collect();

            new_functions.push(FunctionMetricsDiff {
                function_name: after_func.name.clone(),
                metrics,
                is_removed: false,
                is_new: true,
            });
        }
    }

    for before_func in &before_metrics.data {
        if find_function(&after_metrics.data, &before_func.name).is_none() {
            let before_vals =
                build_metrics_from_function(before_func, &before_metrics.percentiles, is_alloc);

            let metrics = before_vals
                .iter()
                .map(|(kind, before_val)| match kind {
                    MetricKind::Calls => MetricDiff::CallsCount(*before_val, 0),
                    MetricKind::Duration => MetricDiff::DurationNs(*before_val, 0),
                    MetricKind::Alloc => MetricDiff::Alloc(*before_val, 0),
                    MetricKind::Percentage => MetricDiff::Percentage(*before_val, 0),
                })
                .collect();

            function_diffs.push(FunctionMetricsDiff {
                function_name: before_func.name.clone(),
                metrics,
                is_removed: true,
                is_new: false,
            });
        }
    }

    function_diffs.extend(new_functions);

    function_diffs.sort_by(|a, b| {
        let a_percent = a
            .metrics
            .iter()
            .find_map(|m| {
                if let MetricDiff::Percentage(_, after) = m {
                    Some(*after)
                } else {
                    None
                }
            })
            .unwrap_or(0);

        let b_percent = b
            .metrics
            .iter()
            .find_map(|m| {
                if let MetricDiff::Percentage(_, after) = m {
                    Some(*after)
                } else {
                    None
                }
            })
            .unwrap_or(0);

        b_percent.cmp(&a_percent)
    });

    MetricsComparison {
        profiling_mode: before_metrics.hotpath_profiling_mode.clone(),
        description: before_metrics.description.clone(),
        percentiles: before_metrics.percentiles.clone(),
        before_elapsed: before_metrics.time_elapsed.clone(),
        after_elapsed: after_metrics.time_elapsed.clone(),
        total_elapsed_diff,
        function_diffs,
    }
}
