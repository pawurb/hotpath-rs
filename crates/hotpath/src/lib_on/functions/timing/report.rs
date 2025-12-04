use std::collections::HashMap;
use std::time::Duration;

use crate::ProfilingMode;

use super::state::FunctionStats;
use crate::output::{MetricType, MetricsProvider};

pub struct StatsData<'a> {
    pub stats: &'a HashMap<&'static str, FunctionStats>,
    pub total_elapsed: Duration,
    pub percentiles: Vec<u8>,
    pub caller_name: &'static str,
    pub limit: usize,
}

impl<'a> MetricsProvider<'a> for StatsData<'a> {
    fn new(
        stats: &'a HashMap<&'static str, FunctionStats>,
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

    fn metric_data(&self) -> HashMap<String, Vec<MetricType>> {
        let wrapper_total = self
            .stats
            .iter()
            .find(|(_, s)| s.wrapper)
            .map(|(_, s)| s.total_duration_ns);

        let reference_total = wrapper_total.unwrap_or(self.total_elapsed.as_nanos() as u64);

        let mut entries: Vec<_> = self.stats.iter().filter(|(_, s)| s.has_data).collect();

        entries.sort_by(|a, b| {
            b.1.total_duration_ns
                .cmp(&a.1.total_duration_ns)
                .then_with(|| a.0.cmp(b.0))
        });

        let entries = if self.limit > 0 {
            entries.into_iter().take(self.limit).collect::<Vec<_>>()
        } else {
            entries
        };

        entries
            .into_iter()
            .map(|(function_name, stats)| {
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

                (function_name.to_string(), metrics)
            })
            .collect()
    }

    fn total_elapsed(&self) -> u64 {
        self.total_elapsed.as_nanos() as u64
    }

    fn caller_name(&self) -> &str {
        self.caller_name
    }

    fn entry_counts(&self) -> (usize, usize) {
        let total_count = self.stats.iter().filter(|(_, s)| s.has_data).count();

        let displayed_count = if self.limit > 0 && self.limit < total_count {
            self.limit
        } else {
            total_count
        };

        (displayed_count, total_count)
    }
}
