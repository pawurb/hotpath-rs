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

struct AllocComputed<'a> {
    stats: &'a FunctionStats,
    total_bytes: u64,
    total_count: u64,
    avg_bytes: u64,
    avg_count: u64,
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
        use crate::lib_on::functions::alloc::guard::{AllocMetric, ALLOC_METRIC};
        match *ALLOC_METRIC {
            AllocMetric::Bytes => ProfilingMode::AllocBytes,
            AllocMetric::Count => ProfilingMode::AllocCount,
        }
    }

    fn description(&self) -> String {
        use crate::lib_on::functions::alloc::guard::{AllocMetric, ALLOC_METRIC};
        let metric = match *ALLOC_METRIC {
            AllocMetric::Bytes => "bytes",
            AllocMetric::Count => "count",
        };
        if *super::guard::ALLOC_SELF {
            format!(
                "Exclusive allocation {} by each function (excluding nested calls).",
                metric
            )
        } else {
            format!(
                "Cumulative allocation {} during each function call (including nested calls).",
                metric
            )
        }
    }

    fn percentiles(&self) -> Vec<u8> {
        self.percentiles.clone()
    }

    fn function_ids(&self) -> HashMap<&'static str, u32> {
        self.stats
            .values()
            .map(|stat| (stat.name, stat.id))
            .collect()
    }

    fn metric_data(&self) -> Vec<(&'static str, Vec<MetricType>)> {
        use crate::lib_on::functions::alloc::guard::{AllocMetric, ALLOC_METRIC};

        let exclude_wrapper = *crate::functions::EXCLUDE_WRAPPER;
        let use_count = *ALLOC_METRIC == AllocMetric::Count;

        let bytes_cache: HashMap<u32, u64> = self
            .stats
            .iter()
            .filter(|(_, s)| s.has_data)
            .map(|(&id, s)| (id, s.total_bytes()))
            .collect();

        let count_cache: HashMap<u32, u64> = self
            .stats
            .iter()
            .filter(|(_, s)| s.has_data)
            .map(|(&id, s)| (id, s.total_count()))
            .collect();

        let mut entries: Vec<AllocComputed> = self
            .stats
            .values()
            .filter(|s| s.has_data && !(exclude_wrapper && s.wrapper))
            .map(|s| {
                let total_bytes = bytes_cache.get(&s.id).copied().unwrap_or(0);
                let total_count = count_cache.get(&s.id).copied().unwrap_or(0);
                let avg_bytes = if s.count > 0 {
                    total_bytes / s.count
                } else {
                    0
                };
                let avg_count = if s.count > 0 {
                    total_count / s.count
                } else {
                    0
                };
                AllocComputed {
                    stats: s,
                    total_bytes,
                    total_count,
                    avg_bytes,
                    avg_count,
                }
            })
            .collect();

        entries.sort_by(|a, b| {
            let primary = if use_count {
                b.total_count.cmp(&a.total_count)
            } else {
                b.total_bytes.cmp(&a.total_bytes)
            };
            primary.then_with(|| a.stats.name.cmp(b.stats.name))
        });

        let entries = if self.limit > 0 {
            entries.into_iter().take(self.limit).collect::<Vec<_>>()
        } else {
            entries
        };

        let primary_cache = if use_count {
            &count_cache
        } else {
            &bytes_cache
        };

        let grand_total: u64 = if *crate::functions::EXCLUDE_WRAPPER {
            self.stats
                .values()
                .filter(|s| !s.wrapper && s.has_data)
                .map(|s| primary_cache.get(&s.id).copied().unwrap_or(0))
                .sum()
        } else if *super::guard::ALLOC_SELF {
            self.stats
                .values()
                .filter(|s| s.has_data)
                .map(|s| primary_cache.get(&s.id).copied().unwrap_or(0))
                .sum()
        } else {
            let wrapper_total = self
                .stats
                .values()
                .find(|s| s.wrapper && s.has_data)
                .map(|s| primary_cache.get(&s.id).copied().unwrap_or(0));

            wrapper_total.unwrap_or_else(|| {
                self.stats
                    .values()
                    .filter(|s| s.has_data)
                    .map(|s| primary_cache.get(&s.id).copied().unwrap_or(0))
                    .sum()
            })
        };

        entries
            .into_iter()
            .map(|entry| {
                let stats = entry.stats;
                let primary_total = if use_count {
                    entry.total_count
                } else {
                    entry.total_bytes
                };
                let percentage = if grand_total > 0 {
                    (primary_total as f64 / grand_total as f64) * 100.0
                } else {
                    0.0
                };

                let mut metrics = if stats.is_async {
                    vec![MetricType::CallsCount(stats.count), MetricType::Unsupported]
                } else {
                    vec![
                        MetricType::CallsCount(stats.count),
                        MetricType::Alloc(entry.avg_bytes, entry.avg_count),
                    ]
                };

                for &p in &self.percentiles {
                    if stats.is_async {
                        metrics.push(MetricType::Unsupported);
                    } else {
                        let bytes_total = stats.bytes_total_percentile(p as f64);
                        let count_total = stats.count_total_percentile(p as f64);
                        metrics.push(MetricType::Alloc(bytes_total, count_total));
                    }
                }

                if stats.is_async {
                    metrics.push(MetricType::Unsupported);
                    metrics.push(MetricType::Unsupported);
                } else {
                    metrics.push(MetricType::Alloc(entry.total_bytes, entry.total_count));
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
