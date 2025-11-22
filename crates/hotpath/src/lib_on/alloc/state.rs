use crossbeam_channel::{Receiver, Sender};
use hdrhistogram::Histogram;
use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub enum Measurement {
    Allocation(&'static str, u64, u64, Duration, bool, bool, bool, u64), // function_name, bytes_total, count_total, elapsed_since_start, unsupported_async, wrapper, cross_thread, tid
}

#[derive(Debug, Clone)]
pub struct FunctionStats {
    pub count: u64,
    bytes_total_hist: Option<Histogram<u64>>,
    count_total_hist: Option<Histogram<u64>>,
    pub has_data: bool,
    pub has_unsupported_async: bool,
    pub wrapper: bool,
    pub cross_thread: bool,
    pub recent_logs: VecDeque<(u64, u64, Duration, u64)>, // (bytes, count, elapsed, tid)
}

impl FunctionStats {
    const LOW_BYTES: u64 = 1;
    const HIGH_BYTES: u64 = 1_000_000_000; // 1GB
    const LOW_COUNT: u64 = 1;
    const HIGH_COUNT: u64 = 1_000_000_000; // 1B allocations
    const SIGFIGS: u8 = 3;

    #[allow(clippy::too_many_arguments)]
    pub fn new_alloc(
        bytes_total: u64,
        count_total: u64,
        elapsed: Duration,
        unsupported_async: bool,
        wrapper: bool,
        cross_thread: bool,
        recent_logs_limit: usize,
        tid: u64,
    ) -> Self {
        let bytes_total_hist =
            Histogram::<u64>::new_with_bounds(Self::LOW_BYTES, Self::HIGH_BYTES, Self::SIGFIGS)
                .expect("bytes_total histogram init");

        let count_total_hist =
            Histogram::<u64>::new_with_bounds(Self::LOW_COUNT, Self::HIGH_COUNT, Self::SIGFIGS)
                .expect("count_total histogram init");

        let mut recent_logs = VecDeque::with_capacity(recent_logs_limit);
        recent_logs.push_back((bytes_total, count_total, elapsed, tid));

        let mut s = Self {
            count: 1,
            bytes_total_hist: Some(bytes_total_hist),
            count_total_hist: Some(count_total_hist),
            has_data: true,
            has_unsupported_async: unsupported_async,
            wrapper,
            cross_thread,
            recent_logs,
        };
        s.record_alloc(bytes_total, count_total);
        s
    }

    #[inline]
    fn record_alloc(&mut self, bytes_total: u64, count_total: u64) {
        if let Some(ref mut bytes_total_hist) = self.bytes_total_hist {
            if bytes_total > 0 {
                let clamped_total = bytes_total.clamp(Self::LOW_BYTES, Self::HIGH_BYTES);
                bytes_total_hist.record(clamped_total).unwrap();
            }
        }
        if let Some(ref mut count_total_hist) = self.count_total_hist {
            if count_total > 0 {
                let clamped_total = count_total.clamp(Self::LOW_COUNT, Self::HIGH_COUNT);
                count_total_hist.record(clamped_total).unwrap();
            }
        }
    }

    pub fn update_alloc(
        &mut self,
        bytes_total: u64,
        count_total: u64,
        elapsed: Duration,
        unsupported_async: bool,
        cross_thread: bool,
        tid: u64,
    ) {
        self.count += 1;
        self.has_unsupported_async |= unsupported_async;
        self.cross_thread |= cross_thread;
        self.record_alloc(bytes_total, count_total);

        if self.recent_logs.len() == self.recent_logs.capacity() && self.recent_logs.capacity() > 0
        {
            self.recent_logs.pop_front();
        }
        self.recent_logs
            .push_back((bytes_total, count_total, elapsed, tid));
    }

    #[inline]
    pub fn bytes_total_percentile(&self, p: f64) -> u64 {
        if self.count == 0 || self.bytes_total_hist.is_none() {
            return 0;
        }
        let p = p.clamp(0.0, 100.0);
        self.bytes_total_hist
            .as_ref()
            .unwrap()
            .value_at_percentile(p)
    }

    #[inline]
    pub fn count_total_percentile(&self, p: f64) -> u64 {
        if self.count == 0 || self.count_total_hist.is_none() {
            return 0;
        }
        let p = p.clamp(0.0, 100.0);
        self.count_total_hist
            .as_ref()
            .unwrap()
            .value_at_percentile(p)
    }

    #[inline]
    pub fn total_bytes(&self) -> u64 {
        if self.count == 0 || self.bytes_total_hist.is_none() {
            return 0;
        }
        // For total bytes allocation, we sum up the mean * count to get total
        let hist = self.bytes_total_hist.as_ref().unwrap();
        let mean = hist.mean();
        (mean * self.count as f64) as u64
    }

    #[inline]
    pub fn avg_bytes(&self) -> u64 {
        if self.count == 0 || self.bytes_total_hist.is_none() {
            return 0;
        }
        self.bytes_total_hist.as_ref().unwrap().mean() as u64
    }

    #[inline]
    pub fn total_count(&self) -> u64 {
        if self.count == 0 || self.count_total_hist.is_none() {
            return 0;
        }
        // For total allocation count, we sum up the mean * count to get total
        let hist = self.count_total_hist.as_ref().unwrap();
        let mean = hist.mean();
        (mean * self.count as f64) as u64
    }

    #[inline]
    pub fn avg_count(&self) -> u64 {
        if self.count == 0 || self.count_total_hist.is_none() {
            return 0;
        }
        self.count_total_hist.as_ref().unwrap().mean() as u64
    }
}

pub(crate) struct HotPathState {
    pub sender: Option<Sender<Measurement>>,
    pub shutdown_tx: Option<Sender<()>>,
    pub completion_rx: Option<Mutex<Receiver<HashMap<&'static str, FunctionStats>>>>,
    pub query_tx: Option<Sender<crate::lib_on::QueryRequest>>,
    pub start_time: Instant,
    pub caller_name: &'static str,
    pub percentiles: Vec<u8>,
    pub limit: usize,
}

pub(crate) fn process_measurement(
    stats: &mut HashMap<&'static str, FunctionStats>,
    m: Measurement,
    recent_logs_limit: usize,
) {
    match m {
        Measurement::Allocation(
            name,
            bytes_total,
            count_total,
            elapsed,
            unsupported_async,
            wrapper,
            cross_thread,
            tid,
        ) => {
            if let Some(s) = stats.get_mut(name) {
                s.update_alloc(
                    bytes_total,
                    count_total,
                    elapsed,
                    unsupported_async,
                    cross_thread,
                    tid,
                );
            } else {
                stats.insert(
                    name,
                    FunctionStats::new_alloc(
                        bytes_total,
                        count_total,
                        elapsed,
                        unsupported_async,
                        wrapper,
                        cross_thread,
                        recent_logs_limit,
                        tid,
                    ),
                );
            }
        }
    }
}

use crate::lib_on::HOTPATH_STATE;

pub fn send_alloc_measurement(
    name: &'static str,
    bytes_total: u64,
    count_total: u64,
    unsupported_async: bool,
    wrapper: bool,
    cross_thread: bool,
    tid: u64,
) {
    let Some(arc_swap) = HOTPATH_STATE.get() else {
        panic!(
            "GuardBuilder::new(\"main\").build() must be called when --features hotpath-alloc is enabled"
        );
    };

    let Some(state) = arc_swap.load_full() else {
        return;
    };

    let Ok(state_guard) = state.read() else {
        return;
    };
    let Some(sender) = state_guard.sender.as_ref() else {
        return;
    };

    let elapsed = state_guard.start_time.elapsed();
    let measurement = Measurement::Allocation(
        name,
        bytes_total,
        count_total,
        elapsed,
        unsupported_async,
        wrapper,
        cross_thread,
        tid,
    );
    let _ = sender.try_send(measurement);
}
