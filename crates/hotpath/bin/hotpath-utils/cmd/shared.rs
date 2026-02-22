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
    pub before_label: Option<String>,
    pub after_label: Option<String>,
    pub total_elapsed_diff: MetricDiff,
    pub cpu_baseline_diff: Option<MetricDiff>,
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

    let (before_section, after_section) = before
        .functions_timing
        .as_ref()
        .zip(after.functions_timing.as_ref())
        .or_else(|| {
            before
                .functions_alloc
                .as_ref()
                .zip(after.functions_alloc.as_ref())
        })
        .unzip();

    let before_ns = before_section.map(|s| s.total_elapsed_ns).unwrap_or(0);
    let after_ns = after_section.map(|s| s.total_elapsed_ns).unwrap_or(0);

    let cpu_baseline_diff = match (&before.cpu_baseline, &after.cpu_baseline) {
        (Some(b), Some(a)) => {
            let b_ns = parse_duration(&b.avg).unwrap_or(0);
            let a_ns = parse_duration(&a.avg).unwrap_or(0);
            Some(MetricDiff::DurationNs(b_ns, a_ns))
        }
        _ => None,
    };

    JsonReportDiff {
        before_label: before.label.clone(),
        after_label: after.label.clone(),
        total_elapsed_diff: MetricDiff::DurationNs(before_ns, after_ns),
        cpu_baseline_diff,
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
        function_diffs,
    }
}

#[cfg(test)]
mod test {
    use crate::cmd::shared::{compare_metrics, compare_reports};
    use hotpath::json::{JsonFunctionEntry, JsonFunctionsList, JsonReport};
    use std::collections::HashMap;

    fn make_function_data(
        name: &str,
        calls: u64,
        avg: u64,
        p95: u64,
        total: u64,
        percent: u64,
    ) -> JsonFunctionEntry {
        let mut percentiles = HashMap::new();
        percentiles.insert("p95".to_string(), hotpath::format_duration(p95));

        JsonFunctionEntry {
            id: 0,
            name: name.to_string(),
            calls,
            avg: hotpath::format_duration(avg),
            percentiles,
            total: hotpath::format_duration(total),
            percent_total: format!("{:.2}%", percent as f64 / 100.0),
        }
    }

    fn make_metrics(data: Vec<JsonFunctionEntry>, total_elapsed_ns: u64) -> JsonFunctionsList {
        JsonFunctionsList {
            hotpath_profiling_mode: hotpath::ProfilingMode::Timing,
            time_elapsed: hotpath::format_duration(total_elapsed_ns),
            total_elapsed_ns,
            total_allocated: None,
            description: "Time metrics".to_string(),
            caller_name: "test::main".to_string(),
            percentiles: vec![95],
            data,
        }
    }

    fn make_alloc_function_data(
        name: &str,
        calls: u64,
        avg_bytes: u64,
        p95_bytes: u64,
        total_bytes: u64,
        percent: u64,
    ) -> JsonFunctionEntry {
        let mut percentiles = HashMap::new();
        percentiles.insert("p95".to_string(), hotpath::format_bytes(p95_bytes));

        JsonFunctionEntry {
            id: 0,
            name: name.to_string(),
            calls,
            avg: hotpath::format_bytes(avg_bytes),
            percentiles,
            total: hotpath::format_bytes(total_bytes),
            percent_total: format!("{:.2}%", percent as f64 / 100.0),
        }
    }

    fn make_alloc_metrics(
        data: Vec<JsonFunctionEntry>,
        total_elapsed_ns: u64,
    ) -> JsonFunctionsList {
        JsonFunctionsList {
            hotpath_profiling_mode: hotpath::ProfilingMode::Alloc,
            time_elapsed: hotpath::format_duration(total_elapsed_ns),
            total_elapsed_ns,
            total_allocated: Some("10.00 MB".to_string()),
            description: "Alloc metrics".to_string(),
            caller_name: "test::main".to_string(),
            percentiles: vec![95],
            data,
        }
    }

    fn make_report(
        timing: Option<JsonFunctionsList>,
        alloc: Option<JsonFunctionsList>,
    ) -> JsonReport {
        JsonReport {
            label: None,
            functions_timing: timing,
            functions_alloc: alloc,
            channels: None,
            streams: None,
            futures: None,
            threads: None,
            cpu_baseline: None,
        }
    }

    #[test]
    fn test_compare_metrics_new_removed_unchanged() {
        let after_data = vec![
            make_function_data("test::function_a", 100, 1000000, 1100000, 100000000, 7000),
            make_function_data("test::function_c", 40, 400000, 450000, 16000000, 1500),
        ];
        let after_metrics = make_metrics(after_data, 140000000);

        let before_data = vec![
            make_function_data("test::function_a", 90, 900000, 1000000, 81000000, 8000),
            make_function_data("test::function_b", 30, 300000, 350000, 9000000, 1200),
        ];
        let before_metrics = make_metrics(before_data, 120000000);

        let comparison = compare_metrics(&before_metrics, &after_metrics);

        assert_eq!(comparison.function_diffs.len(), 3);
        assert!(comparison
            .function_diffs
            .iter()
            .any(|f| f.function_name == "test::function_b" && f.is_removed));
        assert!(comparison
            .function_diffs
            .iter()
            .any(|f| f.function_name == "test::function_c" && f.is_new));
        assert!(comparison
            .function_diffs
            .iter()
            .any(|f| f.function_name == "test::function_a" && !f.is_new && !f.is_removed));
    }

    #[test]
    fn test_compare_reports_timing_and_alloc() {
        let before_timing = make_metrics(
            vec![make_function_data(
                "fn_a", 100, 1000000, 1100000, 100000000, 10000,
            )],
            100000000,
        );
        let after_timing = make_metrics(
            vec![make_function_data(
                "fn_a", 120, 1200000, 1300000, 144000000, 10000,
            )],
            144000000,
        );
        let before_alloc = make_alloc_metrics(
            vec![make_alloc_function_data(
                "fn_a", 100, 1024, 2048, 102400, 10000,
            )],
            100000000,
        );
        let after_alloc = make_alloc_metrics(
            vec![make_alloc_function_data(
                "fn_a", 120, 2048, 4096, 245760, 10000,
            )],
            144000000,
        );

        let before = make_report(Some(before_timing), Some(before_alloc));
        let after = make_report(Some(after_timing), Some(after_alloc));

        let diff = compare_reports(&before, &after);

        assert!(diff.functions_timing.is_some());
        assert!(diff.functions_alloc.is_some());

        let timing = diff.functions_timing.unwrap();
        assert_eq!(timing.function_diffs.len(), 1);
        assert_eq!(timing.function_diffs[0].function_name, "fn_a");

        let alloc = diff.functions_alloc.unwrap();
        assert_eq!(alloc.function_diffs.len(), 1);
        assert_eq!(alloc.function_diffs[0].function_name, "fn_a");
    }
}
