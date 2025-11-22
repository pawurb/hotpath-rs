use crossbeam_channel::{Receiver, Sender};
use hdrhistogram::Histogram;
use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub enum Measurement {
    Duration(u64, Duration, &'static str, bool, u64), // duration_ns, elapsed_since_start, function_name, wrapper, tid
}

#[derive(Debug)]
pub struct FunctionStats {
    pub total_duration_ns: u64,
    pub count: u64,
    hist: Option<Histogram<u64>>,
    pub has_data: bool,
    pub wrapper: bool,
    pub recent_logs: VecDeque<(u64, Duration, u64)>, // (duration_ns, elapsed, tid)
}

impl FunctionStats {
    const LOW_NS: u64 = 1;
    const HIGH_NS: u64 = 1_000_000_000_000; // 1000s
    const SIGFIGS: u8 = 3;

    pub fn new_duration(
        first_ns: u64,
        elapsed: Duration,
        wrapper: bool,
        recent_logs_limit: usize,
        tid: u64,
    ) -> Self {
        let hist = Histogram::<u64>::new_with_bounds(Self::LOW_NS, Self::HIGH_NS, Self::SIGFIGS)
            .expect("hdrhistogram init");

        let mut recent_logs = VecDeque::with_capacity(recent_logs_limit);
        recent_logs.push_back((first_ns, elapsed, tid));

        let mut s = Self {
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

    pub fn update_duration(&mut self, duration_ns: u64, elapsed: Duration, tid: u64) {
        self.total_duration_ns += duration_ns;
        self.count += 1;
        self.record_time(duration_ns);

        if self.recent_logs.len() == self.recent_logs.capacity() && self.recent_logs.capacity() > 0
        {
            self.recent_logs.pop_front();
        }
        self.recent_logs.push_back((duration_ns, elapsed, tid));
    }

    pub fn avg_duration_ns(&self) -> u64 {
        if self.count == 0 {
            0
        } else {
            self.total_duration_ns / self.count
        }
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

pub(crate) struct HotPathState {
    pub sender: Option<Sender<Measurement>>,
    pub shutdown_tx: Option<Sender<()>>,
    pub completion_rx: Option<Mutex<Receiver<HashMap<&'static str, FunctionStats>>>>,
    pub query_tx: Option<Sender<super::super::QueryRequest>>,
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
        Measurement::Duration(duration_ns, elapsed, name, wrapper, tid) => {
            if let Some(s) = stats.get_mut(name) {
                s.update_duration(duration_ns, elapsed, tid);
            } else {
                stats.insert(
                    name,
                    FunctionStats::new_duration(
                        duration_ns,
                        elapsed,
                        wrapper,
                        recent_logs_limit,
                        tid,
                    ),
                );
            }
        }
    }
}

use super::super::HOTPATH_STATE;

pub fn send_duration_measurement(name: &'static str, duration: Duration, wrapper: bool, tid: u64) {
    let Some(arc_swap) = HOTPATH_STATE.get() else {
        panic!(
            "GuardBuilder::new(\"main\").build() must be called when --features hotpath is enabled"
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
    let measurement =
        Measurement::Duration(duration.as_nanos() as u64, elapsed, name, wrapper, tid);
    let _ = sender.try_send(measurement);
}
