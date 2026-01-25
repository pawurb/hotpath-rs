//! Debug subsystem - value logging, debug logging, and gauges.

use crate::channels::{get_log_limit, START_TIME};
use crate::metrics_server::METRICS_SERVER_PORT;
use crossbeam_channel::{unbounded, Sender as CbSender};
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, OnceLock, RwLock};

#[cfg(target_os = "linux")]
use quanta::Instant;

#[cfg(not(target_os = "linux"))]
use std::time::Instant;

pub mod dbg;
pub mod gauge;
pub mod value;

pub use dbg::{get_dbg_logs, get_dbg_stats_json, log_dbg};

#[derive(Debug, Clone)]
pub struct DebugEntry {
    pub index: u64,
    pub timestamp_ns: u64,
    pub value: String,
    pub tid: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct DebugStats {
    pub source: &'static str,
    pub expression: &'static str,
    pub log_count: u64,
    pub logs: VecDeque<DebugEntry>,
}

impl DebugStats {
    fn new(source: &'static str, expression: &'static str) -> Self {
        Self {
            source,
            expression,
            log_count: 0,
            logs: VecDeque::new(),
        }
    }
}

#[derive(Debug)]
pub(crate) enum DebugEvent {
    DbgLog {
        source: &'static str,
        expression: &'static str,
        value: String,
        timestamp: Instant,
        tid: Option<u64>,
    },
}

type DebugState = (
    CbSender<DebugEvent>,
    Arc<RwLock<HashMap<(&'static str, &'static str), DebugStats>>>,
);

static DEBUG_STATE: OnceLock<DebugState> = OnceLock::new();

pub(crate) fn init_debug_state() {
    DEBUG_STATE.get_or_init(|| {
        START_TIME.get_or_init(Instant::now);

        crate::metrics_server::start_metrics_server_once(*METRICS_SERVER_PORT);

        let (event_tx, event_rx) = unbounded::<DebugEvent>();
        let stats_map = Arc::new(RwLock::new(HashMap::<
            (&'static str, &'static str),
            DebugStats,
        >::new()));
        let stats_map_clone = Arc::clone(&stats_map);

        std::thread::Builder::new()
            .name("hp-debug".into())
            .spawn(move || {
                while let Ok(event) = event_rx.recv() {
                    let mut stats = stats_map_clone.write().unwrap();
                    process_debug_event(&mut stats, event);
                }
            })
            .expect("Failed to spawn debug event collector thread");

        (event_tx, stats_map)
    });
}

fn timestamp_nanos(timestamp: Instant) -> u64 {
    let start_time = START_TIME.get().copied().unwrap_or(timestamp);
    timestamp.duration_since(start_time).as_nanos() as u64
}

fn process_debug_event(
    stats_map: &mut HashMap<(&'static str, &'static str), DebugStats>,
    event: DebugEvent,
) {
    let DebugEvent::DbgLog {
        source,
        expression,
        value,
        timestamp,
        tid,
    } = event;

    let key = (source, expression);
    let stats = stats_map
        .entry(key)
        .or_insert_with(|| DebugStats::new(source, expression));

    stats.log_count += 1;

    let entry = DebugEntry {
        index: stats.log_count,
        timestamp_ns: timestamp_nanos(timestamp),
        value,
        tid,
    };

    let limit = get_log_limit();
    if stats.logs.len() >= limit {
        stats.logs.pop_front();
    }
    stats.logs.push_back(entry);
}

pub(crate) fn send_debug_event(event: DebugEvent) {
    if let Some((tx, _)) = DEBUG_STATE.get() {
        let _ = tx.send(event);
    }
}

pub(crate) fn get_all_debug_stats() -> HashMap<(&'static str, &'static str), DebugStats> {
    if let Some((_, stats_map)) = DEBUG_STATE.get() {
        stats_map.read().unwrap().clone()
    } else {
        HashMap::new()
    }
}

pub(crate) fn get_sorted_debug_stats() -> Vec<DebugStats> {
    let mut stats: Vec<DebugStats> = get_all_debug_stats().into_values().collect();
    stats.sort_by(|a, b| a.source.cmp(b.source).then(a.expression.cmp(b.expression)));
    stats
}
