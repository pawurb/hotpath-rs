use crossbeam_channel::{Receiver, Sender};
use hdrhistogram::Histogram;
use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;
use std::time::Duration;

use crate::instant::Instant;

const BATCH_SIZE: usize = 64;
const FLUSH_INTERVAL_MS: u64 = 50;

struct MeasurementBatch {
    measurements: Vec<Measurement>,
    last_flush: Instant,
    sender: Option<Sender<Measurement>>,
    start_time: Option<Instant>,
}

impl MeasurementBatch {
    fn new() -> Self {
        Self {
            measurements: Vec::with_capacity(BATCH_SIZE),
            last_flush: Instant::now(),
            sender: None,
            start_time: None,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn add(
        &mut self,
        name: &'static str,
        bytes_total: Option<u64>,
        count_total: Option<u64>,
        duration: Duration,
        wrapper: bool,
        tid: Option<u64>,
        result_log: Option<String>,
    ) {
        if self.sender.is_none() {
            if let Some(arc_swap) = super::super::FUNCTIONS_STATE.get() {
                if let Some(state) = arc_swap.load_full() {
                    if let Ok(state_guard) = state.read() {
                        self.sender = state_guard.sender.clone();
                        self.start_time = Some(state_guard.start_time);
                    }
                }
            }
        }

        if self.start_time.is_none() {
            return;
        };

        let measurement = Measurement {
            name,
            bytes_total,
            count_total,
            duration,
            measurement_time: Instant::now(),
            wrapper,
            tid,
            result_log,
        };

        self.measurements.push(measurement);

        let should_flush = self.measurements.len() >= BATCH_SIZE
            || self.last_flush.elapsed() >= Duration::from_millis(FLUSH_INTERVAL_MS);

        if should_flush {
            self.flush();
        }
    }

    fn flush(&mut self) {
        if self.measurements.is_empty() {
            return;
        }

        let sender = self.sender.as_ref().expect("Sender must exist");
        for measurement in self.measurements.drain(..) {
            let _ = sender.send(measurement);
        }

        self.last_flush = Instant::now();
    }
}

impl Drop for MeasurementBatch {
    fn drop(&mut self) {
        self.flush();
    }
}

thread_local! {
    static MEASUREMENT_BATCH: RefCell<MeasurementBatch> = RefCell::new(MeasurementBatch::new());
}

pub(crate) fn flush_batch() {
    MEASUREMENT_BATCH.with(|batch| {
        batch.borrow_mut().flush();
    });
}

#[derive(Debug)]
pub(crate) struct Measurement {
    pub(crate) name: &'static str,
    pub(crate) bytes_total: Option<u64>,
    pub(crate) count_total: Option<u64>,
    pub(crate) duration: Duration,
    pub(crate) measurement_time: Instant,
    pub(crate) wrapper: bool,
    pub(crate) tid: Option<u64>,
    pub(crate) result_log: Option<String>,
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
pub(crate) struct FunctionStats {
    pub(crate) id: u32,
    pub(crate) name: &'static str,
    pub(crate) count: u64,
    bytes_total_hist: Option<Histogram<u64>>,
    count_total_hist: Option<Histogram<u64>>,
    duration_hist: Option<Histogram<u64>>,
    pub(crate) total_bytes_sum: u64,
    pub(crate) total_count_sum: u64,
    pub(crate) total_duration_ns: u64,
    pub(crate) has_data: bool,
    pub(crate) is_async: bool,
    pub(crate) wrapper: bool,
    pub(crate) recent_logs: VecDeque<LogEntry>,
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
        id: u32,
        name: &'static str,
        bytes_total: Option<u64>,
        count_total: Option<u64>,
        duration: Duration,
        elapsed: Duration,
        wrapper: bool,
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
        let mut recent_logs = VecDeque::with_capacity(*crate::channels::LOGS_LIMIT);
        recent_logs.push_back((
            bytes_total,
            count_total,
            duration_ns,
            elapsed,
            tid,
            result_log,
        ));

        let mut s = Self {
            id,
            name,
            count: 1,
            bytes_total_hist: Some(bytes_total_hist),
            count_total_hist: Some(count_total_hist),
            duration_hist: Some(duration_hist),
            total_bytes_sum: bytes_total.unwrap_or(0),
            total_count_sum: count_total.unwrap_or(0),
            total_duration_ns: duration_ns,
            has_data: true,
            is_async: bytes_total.is_none(),
            wrapper,
            recent_logs,
        };
        s.record_alloc(bytes_total, count_total);
        s.record_duration(duration_ns);
        s
    }

    #[inline]
    fn record_alloc(&mut self, bytes_total: Option<u64>, count_total: Option<u64>) {
        if let (Some(ref mut bytes_total_hist), Some(bytes)) =
            (&mut self.bytes_total_hist, bytes_total)
        {
            if bytes > 0 {
                let clamped_total = bytes.clamp(Self::LOW_BYTES, Self::HIGH_BYTES);
                bytes_total_hist.record(clamped_total).unwrap();
            }
        }
        if let (Some(ref mut count_total_hist), Some(count)) =
            (&mut self.count_total_hist, count_total)
        {
            if count > 0 {
                let clamped_total = count.clamp(Self::LOW_COUNT, Self::HIGH_COUNT);
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

    pub fn update_alloc(
        &mut self,
        bytes_total: Option<u64>,
        count_total: Option<u64>,
        duration: Duration,
        elapsed: Duration,
        tid: Option<u64>,
        result_log: Option<String>,
    ) {
        self.count += 1;
        self.is_async |= bytes_total.is_none();
        self.total_bytes_sum += bytes_total.unwrap_or(0);
        self.total_count_sum += count_total.unwrap_or(0);
        self.record_alloc(bytes_total, count_total);

        let duration_ns = duration.as_nanos() as u64;
        self.total_duration_ns += duration_ns;
        self.record_duration(duration_ns);

        if self.recent_logs.len() == self.recent_logs.capacity() && self.recent_logs.capacity() > 0
        {
            self.recent_logs.pop_front();
        }
        self.recent_logs.push_back((
            bytes_total,
            count_total,
            duration_ns,
            elapsed,
            tid,
            result_log,
        ));
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
        self.total_bytes_sum
    }

    #[inline]
    pub fn avg_bytes(&self) -> u64 {
        if self.count == 0 {
            return 0;
        }
        self.total_bytes_sum / self.count
    }

    #[inline]
    pub fn total_count(&self) -> u64 {
        self.total_count_sum
    }

    #[inline]
    pub fn avg_count(&self) -> u64 {
        if self.count == 0 {
            return 0;
        }
        self.total_count_sum / self.count
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
    pub completion_rx: Option<Mutex<Receiver<HashMap<u32, FunctionStats>>>>,

    pub start_time: Instant,
    pub caller_name: &'static str,
    pub percentiles: Vec<f64>,
    pub limit: usize,
}

pub(crate) fn process_measurement(
    stats: &mut HashMap<u32, FunctionStats>,
    name_to_id: &mut HashMap<&'static str, u32>,
    m: Measurement,
    start_time: Instant,
) {
    let elapsed = m.measurement_time.duration_since(start_time);
    if let Some(&id) = name_to_id.get(m.name) {
        if let Some(s) = stats.get_mut(&id) {
            s.update_alloc(
                m.bytes_total,
                m.count_total,
                m.duration,
                elapsed,
                m.tid,
                m.result_log,
            );
        }
    } else {
        let id = crate::functions::next_function_id();
        name_to_id.insert(m.name, id);
        stats.insert(
            id,
            FunctionStats::new_alloc(
                id,
                m.name,
                m.bytes_total,
                m.count_total,
                m.duration,
                elapsed,
                m.wrapper,
                m.tid,
                m.result_log,
            ),
        );
    }
}

use super::super::FUNCTIONS_STATE;

pub(crate) fn send_alloc_measurement(
    name: &'static str,
    bytes_total: Option<u64>,
    count_total: Option<u64>,
    duration: Duration,
    wrapper: bool,
    tid: Option<u64>,
) {
    send_alloc_measurement_with_log(name, bytes_total, count_total, duration, wrapper, tid, None);
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn send_alloc_measurement_with_log(
    name: &'static str,
    bytes_total: Option<u64>,
    count_total: Option<u64>,
    duration: Duration,
    wrapper: bool,
    tid: Option<u64>,
    result_log: Option<String>,
) {
    if FUNCTIONS_STATE.get().is_none() {
        return;
    }

    MEASUREMENT_BATCH.with(|batch| {
        batch.borrow_mut().add(
            name,
            bytes_total,
            count_total,
            duration,
            wrapper,
            tid,
            result_log,
        );
    });
}
