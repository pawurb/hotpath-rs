use crate::ProfilingMode;
use std::collections::HashMap;
use std::time::Duration;

use super::state::FunctionStats;
use crate::output::{MetricType, MetricsProvider};

pub struct StatsData<'a> {
    pub stats: &'a HashMap<u32, FunctionStats>,
    pub total_elapsed: Duration,
    pub percentiles: Vec<u8>,
    pub caller_name: &'static str,
    pub limit: usize,
}

pub struct TimingStatsData<'a> {
    pub stats: &'a HashMap<u32, FunctionStats>,
    pub total_elapsed: Duration,
    pub percentiles: Vec<u8>,
    pub caller_name: &'static str,
    pub limit: usize,
}

impl<'a> MetricsProvider<'a> for StatsData<'a> {
    fn new(
        stats: &'a HashMap<u32, FunctionStats>,
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
        if *super::guard::ALLOC_SELF {
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

    fn function_ids(&self) -> HashMap<&'static str, u32> {
        self.stats
            .values()
            .map(|stat| (stat.name, stat.id))
            .collect()
    }

    fn metric_data(&self) -> Vec<(&'static str, Vec<MetricType>)> {
        let exclude_wrapper = *crate::functions::EXCLUDE_WRAPPER;
        let mut entries: Vec<_> = self
            .stats
            .values()
            .filter(|s| {
                s.has_data && !(s.wrapper && s.cross_thread) && !(exclude_wrapper && s.wrapper)
            })
            .collect();

        entries.sort_by(|a, b| {
            b.total_bytes()
                .cmp(&a.total_bytes())
                .then_with(|| a.name.cmp(b.name))
        });

        let entries = if self.limit > 0 {
            entries.into_iter().take(self.limit).collect::<Vec<_>>()
        } else {
            entries
        };

        let grand_total_bytes: u64 = if *crate::functions::EXCLUDE_WRAPPER {
            self.stats
                .values()
                .filter(|s| !s.wrapper && s.has_data)
                .map(|s| s.total_bytes())
                .sum()
        } else if *super::guard::ALLOC_SELF {
            self.stats
                .values()
                .filter(|s| s.has_data)
                .map(|s| s.total_bytes())
                .sum()
        } else {
            let has_cross_thread_wrapper = self.stats.values().any(|s| s.wrapper && s.cross_thread);

            if has_cross_thread_wrapper {
                entries
                    .iter()
                    .filter(|s| !s.wrapper)
                    .map(|s| s.total_bytes())
                    .sum()
            } else {
                let wrapper_total_bytes = self
                    .stats
                    .values()
                    .find(|s| s.wrapper)
                    .map(|s| s.total_bytes());

                wrapper_total_bytes.unwrap_or_else(|| entries.iter().map(|s| s.total_bytes()).sum())
            }
        };

        entries
            .into_iter()
            .map(|stats| {
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

                (stats.name, metrics)
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
        let exclude_wrapper = *crate::functions::EXCLUDE_WRAPPER;
        let total_count = self
            .stats
            .values()
            .filter(|s| {
                s.has_data && !(s.wrapper && s.cross_thread) && !(exclude_wrapper && s.wrapper)
            })
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
        stats: &'a HashMap<u32, FunctionStats>,
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

    fn function_ids(&self) -> HashMap<&'static str, u32> {
        self.stats
            .values()
            .map(|stat| (stat.name, stat.id))
            .collect()
    }

    fn metric_data(&self) -> Vec<(&'static str, Vec<MetricType>)> {
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

        let reference_total = if exclude_wrapper {
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

                for &p in &self.percentiles {
                    let duration_ns = stats.duration_percentile(p as f64);
                    metrics.push(MetricType::DurationNs(duration_ns));
                }

                metrics.push(MetricType::DurationNs(stats.total_duration_ns));
                metrics.push(MetricType::Percentage((percentage * 100.0) as u64));

                (stats.name, metrics)
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
        let exclude_wrapper = *crate::functions::EXCLUDE_WRAPPER;
        let total_count = self
            .stats
            .values()
            .filter(|s| s.has_data && !(exclude_wrapper && s.wrapper))
            .count();

        let displayed_count = if self.limit > 0 && self.limit < total_count {
            self.limit
        } else {
            total_count
        };

        (displayed_count, total_count)
    }
}
