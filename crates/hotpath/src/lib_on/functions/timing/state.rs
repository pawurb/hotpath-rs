use crossbeam_channel::{Receiver, Sender};
use hdrhistogram::Histogram;
use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;
use std::time::Duration;

use crate::functions::batch::{BatchedMeasurement, MeasurementBatch};
use crate::instant::Instant;

thread_local! {
    static MEASUREMENT_BATCH: RefCell<MeasurementBatch<Measurement>> =
        RefCell::new(MeasurementBatch::new());
}

pub(crate) fn flush_batch() {
    MEASUREMENT_BATCH.with(|batch| {
        batch.borrow_mut().flush();
    });
}

#[derive(Debug)]
pub(crate) struct Measurement {
    pub(crate) duration_ns: u64,
    pub(crate) elapsed_since_start_ns: u64,
    pub(crate) name: &'static str,
    pub(crate) wrapper: bool,
    pub(crate) tid: Option<u64>,
    pub(crate) result_log: Option<String>,
}

impl BatchedMeasurement for Measurement {
    fn elapsed_since_start_ns(&self) -> u64 {
        self.elapsed_since_start_ns
    }

    fn fetch_sender() -> Option<Sender<Vec<Self>>> {
        let arc_swap = crate::lib_on::functions::FUNCTIONS_STATE.get()?;
        let state = arc_swap.load_full()?;
        let state_guard = state.read().ok()?;
        state_guard.sender.clone()
    }
}

#[derive(Debug)]
pub(crate) struct FunctionStats {
    pub(crate) id: u32,
    pub(crate) name: &'static str,
    pub(crate) total_duration_ns: u64,
    pub(crate) count: u64,
    hist: Option<Histogram<u64>>,
    pub(crate) has_data: bool,
    pub(crate) wrapper: bool,
    pub(crate) recent_logs: VecDeque<(u64, Duration, Option<u64>, Option<String>)>, // (duration_ns, elapsed, tid, result_log)
}

impl FunctionStats {
    const LOW_NS: u64 = 1;
    const HIGH_NS: u64 = 1_000_000_000_000; // 1000s
    const SIGFIGS: u8 = 3;

    pub fn new_duration(
        id: u32,
        name: &'static str,
        first_ns: u64,
        elapsed: Duration,
        wrapper: bool,
        tid: Option<u64>,
        result_log: Option<String>,
    ) -> Self {
        let hist = Histogram::<u64>::new_with_bounds(Self::LOW_NS, Self::HIGH_NS, Self::SIGFIGS)
            .expect("hdrhistogram init");

        let mut recent_logs = VecDeque::with_capacity(*crate::channels::LOGS_LIMIT);
        recent_logs.push_back((first_ns, elapsed, tid, result_log));

        let mut s = Self {
            id,
            name,
            total_duration_ns: first_ns,
            count: 1,
            hist: Some(hist),
            has_data: true,
            wrapper,
            recent_logs,
        };
        s.record_time(first_ns);
        s
    }

    #[inline]
    fn record_time(&mut self, ns: u64) {
        if let Some(ref mut hist) = self.hist {
            let clamped = ns.clamp(Self::LOW_NS, Self::HIGH_NS);
            hist.record(clamped).unwrap();
        }
    }

    pub fn update_duration(
        &mut self,
        duration_ns: u64,
        elapsed: Duration,
        tid: Option<u64>,
        result_log: Option<String>,
    ) {
        self.total_duration_ns += duration_ns;
        self.count += 1;
        self.record_time(duration_ns);

        if self.recent_logs.len() == self.recent_logs.capacity() && self.recent_logs.capacity() > 0
        {
            self.recent_logs.pop_front();
        }
        self.recent_logs
            .push_back((duration_ns, elapsed, tid, result_log));
    }

    pub fn avg_duration_ns(&self) -> u64 {
        self.total_duration_ns.checked_div(self.count).unwrap_or(0)
    }

    #[inline]
    pub fn percentile(&self, p: f64) -> Duration {
        if self.count == 0 || self.hist.is_none() {
            return Duration::ZERO;
        }
        let p = p.clamp(0.0, 100.0);
        let v = self.hist.as_ref().unwrap().value_at_percentile(p);
        Duration::from_nanos(v)
    }
}

pub(crate) struct FunctionsState {
    pub sender: Option<Sender<Vec<Measurement>>>,
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
) {
    let elapsed = Duration::from_nanos(m.elapsed_since_start_ns);
    if let Some(&id) = name_to_id.get(m.name) {
        if let Some(s) = stats.get_mut(&id) {
            s.update_duration(m.duration_ns, elapsed, m.tid, m.result_log);
        }
    } else {
        let id = crate::functions::next_function_id();
        name_to_id.insert(m.name, id);
        stats.insert(
            id,
            FunctionStats::new_duration(
                id,
                m.name,
                m.duration_ns,
                elapsed,
                m.wrapper,
                m.tid,
                m.result_log,
            ),
        );
    }
}

use crate::lib_on::functions::FUNCTIONS_STATE;

pub(crate) fn send_duration_measurement(
    name: &'static str,
    duration_ns: u64,
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
    duration_ns: u64,
    elapsed_since_start_ns: u64,
    wrapper: bool,
    tid: Option<u64>,
    result_log: Option<String>,
) {
    if FUNCTIONS_STATE.get().is_none() {
        return;
    }

    MEASUREMENT_BATCH.with(|batch| {
        batch.borrow_mut().add(Measurement {
            duration_ns,
            elapsed_since_start_ns,
            name,
            wrapper,
            tid,
            result_log,
        });
    });
}
