//! Debug subsystem - value logging, debug logging, and gauges.

use crate::channels::{get_log_limit, START_TIME};
use crate::metrics_server::METRICS_SERVER_PORT;
use crossbeam_channel::{unbounded, Sender as CbSender};
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock, RwLock};

pub(crate) static DEBUG_ID_COUNTER: AtomicU64 = AtomicU64::new(1);

#[cfg(target_os = "linux")]
use quanta::Instant;

#[cfg(not(target_os = "linux"))]
use std::time::Instant;

pub mod dbg;
pub mod gauge;
pub mod value;

pub use dbg::{get_dbg_logs, get_debug_entries_json, log_dbg};
pub use value::{get_val_logs, log_val};

#[derive(Debug, Clone)]
pub struct DbgEntry {
    pub id: u64,
    pub source: &'static str,
    pub expression: &'static str,
    pub log_count: u64,
    pub logs: VecDeque<DbgLog>,
}

#[derive(Debug, Clone)]
pub struct DbgLog {
    pub index: u64,
    pub timestamp_ns: u64,
    pub value: String,
    pub tid: Option<u64>,
}

impl DbgEntry {
    fn new(id: u64, source: &'static str, expression: &'static str) -> Self {
        Self {
            id,
            source,
            expression,
            log_count: 0,
            logs: VecDeque::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ValEntry {
    pub id: u64,
    pub key: &'static str,
    pub log_count: u64,
    pub logs: VecDeque<ValLog>,
}

#[derive(Debug, Clone)]
pub struct ValLog {
    pub index: u64,
    pub timestamp_ns: u64,
    pub value: String,
    pub source: &'static str,
    pub tid: Option<u64>,
}

impl ValEntry {
    fn new(id: u64, key: &'static str) -> Self {
        Self {
            id,
            key,
            log_count: 0,
            logs: VecDeque::new(),
        }
    }
}

#[derive(Debug)]
pub(crate) enum DebugEvent {
    Dbg {
        source: &'static str,
        expression: &'static str,
        value: String,
        timestamp: Instant,
        tid: Option<u64>,
    },
    Val {
        key: &'static str,
        source: &'static str,
        value: String,
        timestamp: Instant,
        tid: Option<u64>,
    },
}

type DebugState = (
    CbSender<DebugEvent>,
    Arc<RwLock<HashMap<(&'static str, &'static str), DbgEntry>>>,
    Arc<RwLock<HashMap<&'static str, ValEntry>>>,
);

static DEBUG_STATE: OnceLock<DebugState> = OnceLock::new();

pub(crate) fn init_debug_state() {
    DEBUG_STATE.get_or_init(|| {
        START_TIME.get_or_init(Instant::now);

        crate::metrics_server::start_metrics_server_once(*METRICS_SERVER_PORT);

        let (event_tx, event_rx) = unbounded::<DebugEvent>();
        let dbg_stats_map = Arc::new(RwLock::new(
            HashMap::<(&'static str, &'static str), DbgEntry>::new(),
        ));
        let val_stats_map = Arc::new(RwLock::new(HashMap::<&'static str, ValEntry>::new()));
        let dbg_stats_clone = Arc::clone(&dbg_stats_map);
        let val_stats_clone = Arc::clone(&val_stats_map);

        std::thread::Builder::new()
            .name("hp-debug".into())
            .spawn(move || {
                while let Ok(event) = event_rx.recv() {
                    match event {
                        DebugEvent::Dbg { .. } => {
                            let mut stats = dbg_stats_clone.write().unwrap();
                            process_dbg_event(&mut stats, event);
                        }
                        DebugEvent::Val { .. } => {
                            let mut stats = val_stats_clone.write().unwrap();
                            process_val_event(&mut stats, event);
                        }
                    }
                }
            })
            .expect("Failed to spawn debug event collector thread");

        (event_tx, dbg_stats_map, val_stats_map)
    });
}

fn timestamp_nanos(timestamp: Instant) -> u64 {
    let start_time = START_TIME.get().copied().unwrap_or(timestamp);
    timestamp.duration_since(start_time).as_nanos() as u64
}

fn process_dbg_event(
    stats_map: &mut HashMap<(&'static str, &'static str), DbgEntry>,
    event: DebugEvent,
) {
    let DebugEvent::Dbg {
        source,
        expression,
        value,
        timestamp,
        tid,
    } = event
    else {
        return;
    };

    let key = (source, expression);
    let stats = stats_map.entry(key).or_insert_with(|| {
        let id = DEBUG_ID_COUNTER.fetch_add(1, Ordering::Relaxed);
        DbgEntry::new(id, source, expression)
    });

    stats.log_count += 1;

    let entry = DbgLog {
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

fn process_val_event(stats_map: &mut HashMap<&'static str, ValEntry>, event: DebugEvent) {
    let DebugEvent::Val {
        key,
        source,
        value,
        timestamp,
        tid,
    } = event
    else {
        return;
    };

    let stats = stats_map.entry(key).or_insert_with(|| {
        let id = DEBUG_ID_COUNTER.fetch_add(1, Ordering::Relaxed);
        ValEntry::new(id, key)
    });

    stats.log_count += 1;

    let entry = ValLog {
        index: stats.log_count,
        timestamp_ns: timestamp_nanos(timestamp),
        value,
        source,
        tid,
    };

    let limit = get_log_limit();
    if stats.logs.len() >= limit {
        stats.logs.pop_front();
    }
    stats.logs.push_back(entry);
}

pub(crate) fn send_debug_event(event: DebugEvent) {
    if let Some((tx, _, _)) = DEBUG_STATE.get() {
        let _ = tx.send(event);
    }
}

pub(crate) fn get_all_debug_stats() -> HashMap<(&'static str, &'static str), DbgEntry> {
    if let Some((_, dbg_map, _)) = DEBUG_STATE.get() {
        dbg_map.read().unwrap().clone()
    } else {
        HashMap::new()
    }
}

pub(crate) fn get_sorted_debug_stats() -> Vec<DbgEntry> {
    let mut stats: Vec<DbgEntry> = get_all_debug_stats().into_values().collect();
    stats.sort_by(|a, b| a.source.cmp(b.source).then(a.expression.cmp(b.expression)));
    stats
}

pub(crate) fn get_all_value_stats() -> HashMap<&'static str, ValEntry> {
    if let Some((_, _, val_map)) = DEBUG_STATE.get() {
        val_map.read().unwrap().clone()
    } else {
        HashMap::new()
    }
}

pub(crate) fn get_sorted_value_stats() -> Vec<ValEntry> {
    let mut stats: Vec<ValEntry> = get_all_value_stats().into_values().collect();
    stats.sort_by(|a, b| a.key.cmp(b.key));
    stats
}

pub(crate) fn get_debug_stats_by_id(id: u64) -> Option<DbgEntry> {
    get_all_debug_stats()
        .into_values()
        .find(|stats| stats.id == id)
}

pub(crate) fn get_value_stats_by_id(id: u64) -> Option<ValEntry> {
    get_all_value_stats()
        .into_values()
        .find(|stats| stats.id == id)
}
