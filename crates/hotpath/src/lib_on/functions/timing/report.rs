use std::collections::HashMap;

use crate::json::JsonFunctionEntry;
use crate::json::JsonFunctionsList;
use crate::lib_on::functions::StatsConfig;
use crate::output::{format_duration, format_percentile_key, ProfilingMode};

use super::state::FunctionStats;

pub(crate) fn build_functions_list(
    stats: &HashMap<u32, FunctionStats>,
    config: &StatsConfig,
    current_elapsed_ns: u64,
) -> JsonFunctionsList {
    let exclude_wrapper = *crate::functions::EXCLUDE_WRAPPER;

    let reference_total = if exclude_wrapper {
        stats
            .values()
            .filter(|s| !s.wrapper && s.has_data)
            .map(|s| s.total_duration_ns)
            .sum::<u64>()
    } else {
        let wrapper_total = stats
            .values()
            .find(|s| s.wrapper)
            .map(|s| s.total_duration_ns);
        wrapper_total.unwrap_or(config.total_elapsed.as_nanos() as u64)
    };

    let mut entries: Vec<_> = stats
        .values()
        .filter(|s| s.has_data && !(exclude_wrapper && s.wrapper))
        .collect();

    entries.sort_by(|a, b| {
        b.total_duration_ns
            .cmp(&a.total_duration_ns)
            .then_with(|| a.name.cmp(b.name))
    });

    let total_count = entries.len();
    let displayed_count = if config.limit > 0 && config.limit < total_count {
        config.limit
    } else {
        total_count
    };

    if config.limit > 0 {
        entries.truncate(config.limit);
    }

    let data: Vec<JsonFunctionEntry> = entries
        .into_iter()
        .map(|s| {
            let percentage = if reference_total > 0 {
                (s.total_duration_ns as f64 / reference_total as f64) * 100.0
            } else {
                0.0
            };

            let mut percentiles = HashMap::new();
            for &p in &config.percentiles {
                let value = s.percentile(p);
                percentiles.insert(
                    format_percentile_key(p),
                    format_duration(value.as_nanos() as u64),
                );
            }

            JsonFunctionEntry {
                id: s.id,
                name: s.name.to_string(),
                calls: s.count,
                avg: format_duration(s.avg_duration_ns()),
                percentiles,
                total: format_duration(s.total_duration_ns),
                percent_total: format!("{:.2}%", percentage),
            }
        })
        .collect();

    let total_elapsed_ns = config.total_elapsed.as_nanos() as u64;

    JsonFunctionsList {
        profiling_mode: ProfilingMode::Timing,
        time_elapsed: format_duration(total_elapsed_ns),
        total_elapsed_ns: current_elapsed_ns,
        total_allocated: None,
        description: "Execution duration of functions.".to_string(),
        caller_name: config.caller_name.to_string(),
        percentiles: config.percentiles.clone(),
        data,
        displayed_count,
        total_count,
    }
}
