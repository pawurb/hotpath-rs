use crate::ProfilingMode;
use std::collections::HashMap;
use std::time::Duration;

use super::state::FunctionStats;
use crate::output::{MetricType, MetricsProvider};

pub struct StatsData<'a> {
    pub stats: &'a HashMap<&'static str, FunctionStats>,
    pub total_elapsed: Duration,
    pub percentiles: Vec<u8>,
    pub caller_name: &'static str,
    pub limit: usize,
}

pub struct TimingStatsData<'a> {
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

    fn profiling_mode(&self) -> ProfilingMode {
        ProfilingMode::Alloc
    }

    fn description(&self) -> String {
        if super::shared::is_alloc_self_enabled() {
            "Exclusive allocations by each function (excluding nested calls).".to_string()
        } else {
            "Cumulative allocations during each function call (including nested calls).".to_string()
        }
    }

    fn percentiles(&self) -> Vec<u8> {
        self.percentiles.clone()
    }

    fn has_unsupported_async(&self) -> bool {
        self.stats.values().any(|s| s.has_unsupported_async)
    }

    fn metric_data(&self) -> HashMap<String, Vec<MetricType>> {
        let mut filtered_stats: Vec<_> = self
            .stats
            .iter()
            .filter(|(_, s)| s.has_data && !(s.wrapper && s.cross_thread))
            .collect();

        filtered_stats.sort_by(|a, b| {
            b.1.total_bytes()
                .cmp(&a.1.total_bytes())
                .then_with(|| a.0.cmp(b.0))
        });

        let filtered_stats = if self.limit > 0 {
            filtered_stats
                .into_iter()
                .take(self.limit)
                .collect::<Vec<_>>()
        } else {
            filtered_stats
        };

        let grand_total_bytes: u64 = if super::shared::is_alloc_self_enabled() {
            self.stats
                .iter()
                .filter(|(_, s)| s.has_data)
                .map(|(_, stats)| stats.total_bytes())
                .sum()
        } else {
            let has_cross_thread_wrapper =
                self.stats.iter().any(|(_, s)| s.wrapper && s.cross_thread);

            if has_cross_thread_wrapper {
                filtered_stats
                    .iter()
                    .filter(|(_, s)| !s.wrapper)
                    .map(|(_, stats)| stats.total_bytes())
                    .sum()
            } else {
                let wrapper_total_bytes = self
                    .stats
                    .iter()
                    .find(|(_, s)| s.wrapper)
                    .map(|(_, s)| s.total_bytes());

                wrapper_total_bytes.unwrap_or_else(|| {
                    filtered_stats
                        .iter()
                        .map(|(_, stats)| stats.total_bytes())
                        .sum()
                })
            }
        };

        filtered_stats
            .into_iter()
            .map(|(function_name, stats)| {
                let percentage = if grand_total_bytes > 0 {
                    (stats.total_bytes() as f64 / grand_total_bytes as f64) * 100.0
                } else {
                    0.0
                };

                let mut metrics = if stats.has_unsupported_async || stats.cross_thread {
                    vec![MetricType::CallsCount(stats.count), MetricType::Unsupported]
                } else {
                    vec![
                        MetricType::CallsCount(stats.count),
                        MetricType::Alloc(stats.avg_bytes(), stats.avg_count()),
                    ]
                };

                for &p in &self.percentiles {
                    if stats.has_unsupported_async || stats.cross_thread {
                        metrics.push(MetricType::Unsupported);
                    } else {
                        let bytes_total = stats.bytes_total_percentile(p as f64);
                        let count_total = stats.count_total_percentile(p as f64);
                        metrics.push(MetricType::Alloc(bytes_total, count_total));
                    }
                }

                if stats.has_unsupported_async || stats.cross_thread {
                    metrics.push(MetricType::Unsupported);
                    metrics.push(MetricType::Unsupported);
                } else {
                    metrics.push(MetricType::Alloc(stats.total_bytes(), stats.total_count()));
                    metrics.push(MetricType::Percentage((percentage * 100.0) as u64));
                }

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
        let total_count = self
            .stats
            .iter()
            .filter(|(_, s)| s.has_data && !(s.wrapper && s.cross_thread))
            .count();

        let displayed_count = if self.limit > 0 && self.limit < total_count {
            self.limit
        } else {
            total_count
        };

        (displayed_count, total_count)
    }
}

impl<'a> MetricsProvider<'a> for TimingStatsData<'a> {
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

    fn profiling_mode(&self) -> ProfilingMode {
        ProfilingMode::Timing
    }

    fn description(&self) -> String {
        "Function execution time metrics.".to_string()
    }

    fn percentiles(&self) -> Vec<u8> {
        self.percentiles.clone()
    }

    fn has_unsupported_async(&self) -> bool {
        false
    }

    fn metric_data(&self) -> HashMap<String, Vec<MetricType>> {
        let mut filtered_stats: Vec<_> = self.stats.iter().filter(|(_, s)| s.has_data).collect();

        filtered_stats.sort_by(|a, b| {
            b.1.total_duration_ns
                .cmp(&a.1.total_duration_ns)
                .then_with(|| a.0.cmp(b.0))
        });

        let filtered_stats = if self.limit > 0 {
            filtered_stats
                .into_iter()
                .take(self.limit)
                .collect::<Vec<_>>()
        } else {
            filtered_stats
        };

        let wrapper_total = self
            .stats
            .iter()
            .find(|(_, s)| s.wrapper)
            .map(|(_, s)| s.total_duration_ns);

        let reference_total = wrapper_total.unwrap_or(self.total_elapsed.as_nanos() as u64);

        filtered_stats
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

                for &p in &self.percentiles {
                    let duration_ns = stats.duration_percentile(p as f64);
                    metrics.push(MetricType::DurationNs(duration_ns));
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
