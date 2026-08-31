use crossbeam_channel::Receiver;
use hdrhistogram::Histogram;
use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;
use std::time::Duration;

use crate::batch::{EventProducer, EventQueueRegistry};
use crate::instant::Instant;

static MEASUREMENT_QUEUES: EventQueueRegistry<Measurement> = EventQueueRegistry::new();

thread_local! {
    static MEASUREMENT_PRODUCER: EventProducer<Measurement> = MEASUREMENT_QUEUES.register();
}

fn add_measurement(m: Measurement) {
    if !MEASUREMENT_QUEUES.is_active() {
        return;
    }
    let _ = MEASUREMENT_PRODUCER.try_with(|producer| producer.push(m));
}

pub(crate) fn set_measurements_active(active: bool) {
    MEASUREMENT_QUEUES.set_active(active);
}

pub(crate) fn sweep_measurements(out: &mut Vec<Measurement>) {
    MEASUREMENT_QUEUES.sweep(out);
}

pub(crate) fn drain_all_measurements(out: &mut Vec<Measurement>) {
    MEASUREMENT_QUEUES.drain_all(out);
}

#[derive(Debug)]
pub(crate) struct Measurement {
    pub(crate) duration_ns: Option<u64>,
    pub(crate) elapsed_since_start_ns: u64,
    pub(crate) name: &'static str,
    pub(crate) wrapper: bool,
    pub(crate) tid: Option<u64>,
    pub(crate) result_log: Option<String>,
}

/// (duration_ns, elapsed, tid, result_log); `duration_ns` is `None` when time
/// sampling skipped the call.
pub(crate) type TimingLogEntry = (Option<u64>, Duration, Option<u64>, Option<String>);

#[derive(Debug)]
pub(crate) struct FunctionStats {
    pub(crate) id: u32,
    pub(crate) name: &'static str,
    pub(crate) total_duration_ns: u64,
    pub(crate) count: u64,
    pub(crate) sampled_count: u64,
    hist: Option<Histogram<u64>>,
    pub(crate) has_data: bool,
    pub(crate) wrapper: bool,
    pub(crate) recent_logs: VecDeque<TimingLogEntry>,
}

impl FunctionStats {
    const LOW_NS: u64 = 1;
    const HIGH_NS: u64 = crate::lib_on::MAX_DURATION_NS;
    const SIGFIGS: u8 = 3;

    fn new(id: u32, name: &'static str, wrapper: bool) -> Self {
        let hist = Histogram::<u64>::new_with_bounds(Self::LOW_NS, Self::HIGH_NS, Self::SIGFIGS)
            .expect("hdrhistogram init");

        Self {
            id,
            name,
            total_duration_ns: 0,
            count: 0,
            sampled_count: 0,
            hist: Some(hist),
            has_data: false,
            wrapper,
            recent_logs: VecDeque::with_capacity(*crate::channels::LOGS_LIMIT),
        }
    }

    #[inline]
    fn record_time(&mut self, ns: u64) {
        if let Some(ref mut hist) = self.hist {
            let clamped = ns.clamp(Self::LOW_NS, Self::HIGH_NS);
            hist.record(clamped).unwrap();
        }
    }

    pub fn update(
        &mut self,
        duration_ns: Option<u64>,
        elapsed: Duration,
        tid: Option<u64>,
        result_log: Option<String>,
    ) {
        self.count += 1;
        self.has_data = true;

        if let Some(duration_ns) = duration_ns {
            self.sampled_count += 1;
            self.total_duration_ns += duration_ns;
            self.record_time(duration_ns);
        }

        if self.recent_logs.len() >= *crate::channels::LOGS_LIMIT {
            self.recent_logs.pop_front();
        }
        self.recent_logs
            .push_back((duration_ns, elapsed, tid, result_log));
    }

    pub fn avg_duration_ns(&self) -> u64 {
        self.total_duration_ns
            .checked_div(self.sampled_count)
            .unwrap_or(0)
    }

    /// Exact when every call was timed, extrapolated (`avg * count`) under time sampling.
    pub fn display_total_ns(&self) -> u64 {
        if self.sampled_count == self.count {
            self.total_duration_ns
        } else {
            self.avg_duration_ns() * self.count
        }
    }

    #[inline]
    pub fn percentile(&self, p: f64) -> Duration {
        if self.sampled_count == 0 || self.hist.is_none() {
            return Duration::ZERO;
        }
        let p = p.clamp(0.0, 100.0);
        let v = self.hist.as_ref().unwrap().value_at_percentile(p);
        Duration::from_nanos(v)
    }

    pub(crate) fn histogram_base64(&self) -> Option<String> {
        if self.sampled_count == 0 {
            return None;
        }
        crate::lib_on::histograms::histogram_base64(self.hist.as_ref()?)
    }

    /// Sparse native-histogram buckets of sampled durations, `(index, count)`
    /// at `schema`, for the Prometheus exporter.
    #[cfg(feature = "hotpath-prometheus-meta")]
    pub(crate) fn native_duration_buckets(&self, schema: i32) -> Vec<(i32, u64)> {
        match self.hist.as_ref().filter(|_| self.sampled_count > 0) {
            Some(hist) => crate::lib_on::native_histograms::native_bucket_counts(
                hist,
                schema,
                crate::lib_on::native_histograms::NANOS_SCALE,
            ),
            None => Vec::new(),
        }
    }

    /// Cumulative classic-bucket counts of sampled durations at or below each
    /// boundary (ns), exact to the histogram's 0.1% resolution.
    #[cfg(feature = "hotpath-prometheus-meta")]
    pub(crate) fn classic_duration_buckets(&self, boundaries: &[u64]) -> Vec<u64> {
        match self.hist.as_ref().filter(|_| self.sampled_count > 0) {
            Some(hist) => {
                crate::lib_on::native_histograms::cumulative_bucket_counts(hist, boundaries)
            }
            None => vec![0; boundaries.len()],
        }
    }
}

pub(crate) struct FunctionsState {
    pub shutdown_tx: Option<crossbeam_channel::Sender<()>>,
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
) {
    let elapsed = Duration::from_nanos(m.elapsed_since_start_ns);
    let id = match name_to_id.get(m.name) {
        Some(&id) => id,
        None => {
            let id = crate::functions::next_function_id();
            name_to_id.insert(m.name, id);
            stats.insert(id, FunctionStats::new(id, m.name, m.wrapper));
            id
        }
    };
    if let Some(s) = stats.get_mut(&id) {
        s.update(m.duration_ns, elapsed, m.tid, m.result_log);
    }
}

use crate::lib_on::functions::FUNCTIONS_STATE;

pub(crate) fn send_duration_measurement(
    name: &'static str,
    duration_ns: Option<u64>,
    elapsed_since_start_ns: u64,
    wrapper: bool,
    tid: Option<u64>,
) {
    send_duration_measurement_with_log(
        name,
        duration_ns,
        elapsed_since_start_ns,
        wrapper,
        tid,
        None,
    );
}

pub(crate) fn send_duration_measurement_with_log(
    name: &'static str,
    duration_ns: Option<u64>,
    elapsed_since_start_ns: u64,
    wrapper: bool,
    tid: Option<u64>,
    result_log: Option<String>,
) {
    if FUNCTIONS_STATE.get().is_none() {
        return;
    }

    add_measurement(Measurement {
        duration_ns,
        elapsed_since_start_ns,
        name,
        wrapper,
        tid,
        result_log,
    });
}
