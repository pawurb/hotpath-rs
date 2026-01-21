use serde::Serialize;
use std::collections::HashMap;

use crate::output::{
    format_bytes, format_duration, FunctionLogsJson, FunctionsJson, MetricType, ProfilingMode,
};

#[derive(Debug, Clone, Serialize)]
pub(crate) struct FunctionMCPData {
    pub name: String,
    pub calls: u64,
    pub avg: String,
    #[serde(flatten)]
    pub percentiles: HashMap<String, String>,
    pub total: String,
    pub percent_total: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct FunctionsMCPJson {
    pub profiling_mode: String,
    pub total_elapsed: String,
    pub description: String,
    pub caller_name: String,
    pub data: Vec<FunctionMCPData>,
}

impl From<&FunctionsJson> for FunctionsMCPJson {
    fn from(json: &FunctionsJson) -> Self {
        let is_alloc = matches!(json.hotpath_profiling_mode, ProfilingMode::Alloc);

        let format_value = |metric: &MetricType| -> String {
            match metric {
                MetricType::DurationNs(ns) => format_duration(*ns),
                MetricType::Alloc(bytes, _) => format_bytes(*bytes),
                MetricType::Unsupported => "N/A".to_string(),
                _ => metric.to_string(),
            }
        };

        let data = json
            .data
            .iter()
            .map(|(name, metrics)| {
                let calls = match &metrics[0] {
                    MetricType::CallsCount(c) => *c,
                    _ => 0,
                };
                let avg = format_value(&metrics[1]);

                let mut percentiles = HashMap::new();
                for (i, &p) in json.percentiles.iter().enumerate() {
                    let metric_idx = 2 + i;
                    if metric_idx < metrics.len() - 2 {
                        percentiles.insert(format!("p{}", p), format_value(&metrics[metric_idx]));
                    }
                }

                let total_idx = metrics.len() - 2;
                let percent_idx = metrics.len() - 1;

                let total = format_value(&metrics[total_idx]);
                let percent_total = match &metrics[percent_idx] {
                    MetricType::Percentage(bp) => format!("{:.2}%", *bp as f64 / 100.0),
                    MetricType::Unsupported => "N/A".to_string(),
                    _ => "0%".to_string(),
                };

                FunctionMCPData {
                    name: name.clone(),
                    calls,
                    avg,
                    percentiles,
                    total,
                    percent_total,
                }
            })
            .collect();

        FunctionsMCPJson {
            profiling_mode: json.hotpath_profiling_mode.to_string(),
            total_elapsed: if is_alloc {
                format_bytes(json.total_elapsed)
            } else {
                format_duration(json.total_elapsed)
            },
            description: json.description.clone(),
            caller_name: json.caller_name.clone(),
            data,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct FunctionTimingLogEntry {
    pub duration: String,
    pub timestamp: String,
    pub thread_id: Option<u64>,
    pub result: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct FunctionTimingLogsMCPJson {
    pub function_name: String,
    pub total_invocations: usize,
    pub logs: Vec<FunctionTimingLogEntry>,
}

impl From<&FunctionLogsJson> for FunctionTimingLogsMCPJson {
    fn from(json: &FunctionLogsJson) -> Self {
        let logs = json
            .logs
            .iter()
            .map(|entry| {
                let duration = entry
                    .value
                    .map(format_duration)
                    .unwrap_or_else(|| "N/A".to_string());

                let timestamp = format_duration(entry.elapsed_nanos);

                FunctionTimingLogEntry {
                    duration,
                    timestamp,
                    thread_id: entry.tid,
                    result: entry.result.clone(),
                }
            })
            .collect();

        FunctionTimingLogsMCPJson {
            function_name: json.function_name.clone(),
            total_invocations: json.count,
            logs,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct FunctionAllocLogEntry {
    pub bytes: String,
    pub timestamp: String,
    pub thread_id: Option<u64>,
    pub result: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct FunctionAllocLogsMCPJson {
    pub function_name: String,
    pub total_invocations: usize,
    pub logs: Vec<FunctionAllocLogEntry>,
}

impl From<&FunctionLogsJson> for FunctionAllocLogsMCPJson {
    fn from(json: &FunctionLogsJson) -> Self {
        let logs = json
            .logs
            .iter()
            .map(|entry| {
                let bytes = entry
                    .value
                    .map(format_bytes)
                    .unwrap_or_else(|| "N/A".to_string());

                let timestamp = format_duration(entry.elapsed_nanos);

                FunctionAllocLogEntry {
                    bytes,
                    timestamp,
                    thread_id: entry.tid,
                    result: entry.result.clone(),
                }
            })
            .collect();

        FunctionAllocLogsMCPJson {
            function_name: json.function_name.clone(),
            total_invocations: json.count,
            logs,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_alloc_mode_formatting() {
        // Raw input (what LLMs struggle to interpret):
        // {
        //   "hotpath_profiling_mode": "alloc",
        //   "total_elapsed": 1394730364208,
        //   "data": [{ "name": "render_ui", "calls": 5178, "avg": 60437, "p95": 60447, "total": 312947932, "percent_total": 3884 }]
        // }
        let raw = FunctionsJson {
            hotpath_profiling_mode: ProfilingMode::Alloc,
            total_elapsed: 1394730364208,
            description: "Cumulative allocations".to_string(),
            caller_name: "hotpath::main".to_string(),
            percentiles: vec![95],
            data: vec![(
                "render_ui".to_string(),
                vec![
                    MetricType::CallsCount(5178),
                    MetricType::Alloc(60437, 0),
                    MetricType::Alloc(60447, 0),
                    MetricType::Alloc(312947932, 0),
                    MetricType::Percentage(3884),
                ],
            )],
        };

        let formatted = FunctionsMCPJson::from(&raw);

        // Formatted output (human/LLM readable):
        // {
        //   "profiling_mode": "alloc",
        //   "total_elapsed": "1.3 TB",
        //   "data": [{ "name": "render_ui", "calls": 5178, "avg": "59.0 KB", "p95": "59.0 KB", "total": "298.5 MB", "percent_total": "38.84%" }]
        // }
        assert_eq!(formatted.profiling_mode, "alloc");
        assert_eq!(formatted.total_elapsed, "1.3 TB");
        assert_eq!(formatted.data[0].calls, 5178);
        assert_eq!(formatted.data[0].avg, "59.0 KB");
        assert_eq!(formatted.data[0].percentiles.get("p95").unwrap(), "59.0 KB");
        assert_eq!(formatted.data[0].total, "298.5 MB");
        assert_eq!(formatted.data[0].percent_total, "38.84%");
    }
}
