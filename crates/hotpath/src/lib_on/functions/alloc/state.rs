use crossbeam_channel::{Receiver, Sender};
use hdrhistogram::Histogram;
use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub struct Measurement {
    pub name: &'static str,
    pub bytes_total: u64,
    pub count_total: u64,
    pub duration: Duration,
    pub elapsed_since_start: Duration,
    pub unsupported_async: bool,
    pub wrapper: bool,
    pub cross_thread: bool,
    pub tid: Option<u64>,
    pub result_log: Option<String>,
}

type LogEntry = (
    Option<u64>,
    Option<u64>,
    u64,
    Duration,
    Option<u64>,
    Option<String>,
);

#[derive(Debug, Clone)]
pub struct FunctionStats {
    pub count: u64,
    bytes_total_hist: Option<Histogram<u64>>,
    count_total_hist: Option<Histogram<u64>>,
    duration_hist: Option<Histogram<u64>>,
    pub total_duration_ns: u64,
    pub has_data: bool,
    pub has_unsupported_async: bool,
    pub wrapper: bool,
    pub cross_thread: bool,
    pub recent_logs: VecDeque<LogEntry>,
}

impl FunctionStats {
    const LOW_BYTES: u64 = 1;
    const HIGH_BYTES: u64 = 1_000_000_000; // 1GB
    const LOW_COUNT: u64 = 1;
    const HIGH_COUNT: u64 = 1_000_000_000;
    const LOW_DURATION_NS: u64 = 1;
    const HIGH_DURATION_NS: u64 = 3_600_000_000_000; // 1 hour in nanoseconds
    const SIGFIGS: u8 = 3;

    #[allow(clippy::too_many_arguments)]
    pub fn new_alloc(
        bytes_total: u64,
        count_total: u64,
        duration: Duration,
        elapsed: Duration,
        unsupported_async: bool,
        wrapper: bool,
        cross_thread: bool,
        recent_logs_limit: usize,
        tid: Option<u64>,
        result_log: Option<String>,
    ) -> Self {
        let bytes_total_hist =
            Histogram::<u64>::new_with_bounds(Self::LOW_BYTES, Self::HIGH_BYTES, Self::SIGFIGS)
                .expect("bytes_total histogram init");

        let count_total_hist =
            Histogram::<u64>::new_with_bounds(Self::LOW_COUNT, Self::HIGH_COUNT, Self::SIGFIGS)
                .expect("count_total histogram init");

        let duration_hist = Histogram::<u64>::new_with_bounds(
            Self::LOW_DURATION_NS,
            Self::HIGH_DURATION_NS,
            Self::SIGFIGS,
        )
        .expect("duration histogram init");

        let duration_ns = duration.as_nanos() as u64;
        let mut recent_logs = VecDeque::with_capacity(recent_logs_limit);
        let (bytes_opt, count_opt) = if unsupported_async || cross_thread {
            (None, None)
        } else {
            (Some(bytes_total), Some(count_total))
        };
        recent_logs.push_back((bytes_opt, count_opt, duration_ns, elapsed, tid, result_log));

        let mut s = Self {
            count: 1,
            bytes_total_hist: Some(bytes_total_hist),
            count_total_hist: Some(count_total_hist),
            duration_hist: Some(duration_hist),
            total_duration_ns: duration_ns,
            has_data: true,
            has_unsupported_async: unsupported_async,
            wrapper,
            cross_thread,
            recent_logs,
        };
        s.record_alloc(bytes_total, count_total);
        s.record_duration(duration_ns);
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

    #[inline]
    fn record_duration(&mut self, duration_ns: u64) {
        if let Some(ref mut duration_hist) = self.duration_hist {
            if duration_ns > 0 {
                let clamped_duration =
                    duration_ns.clamp(Self::LOW_DURATION_NS, Self::HIGH_DURATION_NS);
                duration_hist.record(clamped_duration).unwrap();
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn update_alloc(
        &mut self,
        bytes_total: u64,
        count_total: u64,
        duration: Duration,
        elapsed: Duration,
        unsupported_async: bool,
        cross_thread: bool,
        tid: Option<u64>,
        result_log: Option<String>,
    ) {
        self.count += 1;
        self.has_unsupported_async |= unsupported_async;
        self.cross_thread |= cross_thread;
        self.record_alloc(bytes_total, count_total);

        let duration_ns = duration.as_nanos() as u64;
        self.total_duration_ns += duration_ns;
        self.record_duration(duration_ns);

        if self.recent_logs.len() == self.recent_logs.capacity() && self.recent_logs.capacity() > 0
        {
            self.recent_logs.pop_front();
        }
        let (bytes_opt, count_opt) = if unsupported_async || cross_thread {
            (None, None)
        } else {
            (Some(bytes_total), Some(count_total))
        };
        self.recent_logs
            .push_back((bytes_opt, count_opt, duration_ns, elapsed, tid, result_log));
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

    #[inline]
    pub fn duration_percentile(&self, p: f64) -> u64 {
        if self.count == 0 || self.duration_hist.is_none() {
            return 0;
        }
        let p = p.clamp(0.0, 100.0);
        self.duration_hist.as_ref().unwrap().value_at_percentile(p)
    }

    #[inline]
    pub fn avg_duration_ns(&self) -> u64 {
        if self.count == 0 || self.duration_hist.is_none() {
            return 0;
        }
        self.duration_hist.as_ref().unwrap().mean() as u64
    }
}

pub(crate) struct FunctionsState {
    pub sender: Option<Sender<Measurement>>,
    pub shutdown_tx: Option<Sender<()>>,
    pub completion_rx: Option<Mutex<Receiver<HashMap<&'static str, FunctionStats>>>>,
    pub query_tx: Option<Sender<super::super::FunctionsQuery>>,
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
    if let Some(s) = stats.get_mut(m.name) {
        s.update_alloc(
            m.bytes_total,
            m.count_total,
            m.duration,
            m.elapsed_since_start,
            m.unsupported_async,
            m.cross_thread,
            m.tid,
            m.result_log,
        );
    } else {
        stats.insert(
            m.name,
            FunctionStats::new_alloc(
                m.bytes_total,
                m.count_total,
                m.duration,
                m.elapsed_since_start,
                m.unsupported_async,
                m.wrapper,
                m.cross_thread,
                recent_logs_limit,
                m.tid,
                m.result_log,
            ),
        );
    }
}

use super::super::FUNCTIONS_STATE;

#[allow(clippy::too_many_arguments)]
pub fn send_alloc_measurement(
    name: &'static str,
    bytes_total: u64,
    count_total: u64,
    duration: Duration,
    unsupported_async: bool,
    wrapper: bool,
    cross_thread: bool,
    tid: Option<u64>,
) {
    send_alloc_measurement_with_log(
        name,
        bytes_total,
        count_total,
        duration,
        unsupported_async,
        wrapper,
        cross_thread,
        tid,
        None,
    );
}

#[allow(clippy::too_many_arguments)]
pub fn send_alloc_measurement_with_log(
    name: &'static str,
    bytes_total: u64,
    count_total: u64,
    duration: Duration,
    unsupported_async: bool,
    wrapper: bool,
    cross_thread: bool,
    tid: Option<u64>,
    result_log: Option<String>,
) {
    let Some(arc_swap) = FUNCTIONS_STATE.get() else {
        panic!(
            "FunctionsGuardBuilder::new(\"main\").build() or #[hotpath::main] must be used when --features hotpath-alloc is enabled"
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
    let measurement = Measurement {
        name,
        bytes_total,
        count_total,
        duration,
        elapsed_since_start: elapsed,
        unsupported_async,
        wrapper,
        cross_thread,
        tid,
        result_log,
    };
    let _ = sender.try_send(measurement);
}
