use std::collections::HashMap;
use std::time::Duration;

use crate::output::{MetricType, MetricsProvider};
use crate::ProfilingMode;

use super::state::FunctionStats;

pub struct StatsData<'a> {
    pub stats: &'a HashMap<u64, FunctionStats>,
    pub total_elapsed: Duration,
    pub percentiles: Vec<u8>,
    pub caller_name: &'static str,
    pub limit: usize,
}

impl<'a> MetricsProvider<'a> for StatsData<'a> {
    fn new(
        stats: &'a HashMap<u64, FunctionStats>,
        total_elapsed: Duration,
        percentiles: Vec<u8>,
        caller_name: &'static str,
        limit: usize,
    ) -> Self {
        Self {
            stats,
            total_elapsed,
            percentiles,
            caller_name,
            limit,
        }
    }

    fn percentiles(&self) -> Vec<u8> {
        self.percentiles.clone()
    }

    fn description(&self) -> String {
        "Execution duration of functions.".to_string()
    }

    fn profiling_mode(&self) -> ProfilingMode {
        ProfilingMode::Timing
    }

    fn metric_data(&self) -> Vec<(String, Vec<MetricType>)> {
        let reference_total = if *crate::functions::EXCLUDE_WRAPPER {
            self.stats
                .values()
                .filter(|s| !s.wrapper && s.has_data)
                .map(|s| s.total_duration_ns)
                .sum::<u64>()
        } else {
            let wrapper_total = self
                .stats
                .values()
                .find(|s| s.wrapper)
                .map(|s| s.total_duration_ns);
            wrapper_total.unwrap_or(self.total_elapsed.as_nanos() as u64)
        };

        let exclude_wrapper = *crate::functions::EXCLUDE_WRAPPER;
        let mut entries: Vec<_> = self
            .stats
            .values()
            .filter(|s| s.has_data && !(exclude_wrapper && s.wrapper))
            .collect();

        entries.sort_by(|a, b| {
            b.total_duration_ns
                .cmp(&a.total_duration_ns)
                .then_with(|| a.name.cmp(b.name))
        });

        let entries = if self.limit > 0 {
            entries.into_iter().take(self.limit).collect::<Vec<_>>()
        } else {
            entries
        };

        entries
            .into_iter()
            .map(|stats| {
                let percentage = if reference_total > 0 {
                    (stats.total_duration_ns as f64 / reference_total as f64) * 100.0
                } else {
                    0.0
                };

                let mut metrics = vec![
                    MetricType::CallsCount(stats.count),
                    MetricType::DurationNs(stats.avg_duration_ns()),
                ];

                for p in self.percentiles.iter() {
                    let value = stats.percentile(*p as f64);
                    metrics.push(MetricType::DurationNs(value.as_nanos() as u64));
                }

                metrics.push(MetricType::DurationNs(stats.total_duration_ns));
                metrics.push(MetricType::Percentage((percentage * 100.0) as u64));

                (stats.name.to_string(), metrics)
            })
            .collect()
    }

    fn function_ids(&self) -> HashMap<String, u64> {
        self.stats
            .values()
            .map(|stat| (stat.name.to_string(), stat.id))
            .collect()
    }

    fn total_elapsed(&self) -> u64 {
        self.total_elapsed.as_nanos() as u64
    }

    fn caller_name(&self) -> &str {
        self.caller_name
    }

    fn entry_counts(&self) -> (usize, usize) {
        let exclude_wrapper = *crate::functions::EXCLUDE_WRAPPER;
        let total_count = self
            .stats
            .iter()
            .filter(|(_, s)| s.has_data && !(exclude_wrapper && s.wrapper))
            .count();

        let displayed_count = if self.limit > 0 && self.limit < total_count {
            self.limit
        } else {
            total_count
        };

        (displayed_count, total_count)
    }
}
