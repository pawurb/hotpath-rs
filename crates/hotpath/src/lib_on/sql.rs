//! SQL query instrumentation module - tracks query execution durations.
//!
//! Unlike the lock subsystems (which key statistics by call site), SQL entries
//! are keyed by *normalized* query text, so parameter-varied executions of the
//! same statement merge into a single bucket (see [`normalize`]). Normalization
//! runs on the background worker thread to keep the hot path light.
//!
//! The write path (worker, events, normalization) is driven by front-ends -
//! the `sqlx` tracing layer (see [`tracing_layer`]) and the Diesel
//! instrumentation (see [`diesel`]) - so it is dead when neither feature is on;
//! the read path stays compiled so the report/metrics wiring is feature-uniform.
#![cfg_attr(not(any(feature = "sqlx", feature = "diesel")), allow(dead_code))]

use crossbeam_channel::{bounded, Receiver as CbReceiver, RecvTimeoutError, Sender as CbSender};
use hdrhistogram::Histogram;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex as StdMutex, OnceLock, RwLock as StdRwLock};

use crate::batch::{EventProducer, EventQueueRegistry};
use crate::instant::Instant;
use crate::lib_on::hotpath_guard::DRAIN_INTERVAL_MS;
use crate::lib_on::START_TIME;
use crate::metrics_server::METRICS_SERVER_PORT;

#[cfg(feature = "diesel")]
pub(crate) mod diesel;
pub(crate) mod normalize;
#[cfg(feature = "sqlx")]
pub(crate) mod tracing_layer;

#[cfg(feature = "diesel")]
pub use diesel::instrument_diesel_sql;
#[cfg(feature = "sqlx")]
pub use tracing_layer::sqlx_tracing_layer;

static SQL_ID_COUNTER: AtomicU32 = AtomicU32::new(1);

fn next_sql_id() -> u32 {
    SQL_ID_COUNTER.fetch_add(1, Ordering::Relaxed)
}

/// Events sent to the background SQL statistics collection thread.
#[derive(Debug)]
pub(crate) enum SqlEvent {
    /// Emitted when an executed query (or query stream) completes. `sql` is the
    /// raw statement text; the worker normalizes it to derive the bucket key.
    Executed { sql: Arc<str>, duration_nanos: u64 },
}

/// Aggregated statistics for a single normalized query.
#[derive(Debug, Clone)]
pub(crate) struct SqlEntry {
    pub(crate) id: u32,
    pub(crate) query: String,
    pub(crate) count: u64,
    pub(crate) total_nanos: u64,
    hist: Option<Histogram<u64>>,
}

impl SqlEntry {
    const LOW_NS: u64 = 1;
    const HIGH_NS: u64 = 1_000_000_000_000; // 1000s
    const SIGFIGS: u8 = 3;

    fn new(id: u32, query: String) -> Self {
        Self {
            id,
            query,
            count: 0,
            total_nanos: 0,
            hist: Histogram::<u64>::new_with_bounds(Self::LOW_NS, Self::HIGH_NS, Self::SIGFIGS)
                .ok(),
        }
    }

    #[inline]
    fn record(&mut self, nanos: u64) {
        if let Some(ref mut hist) = self.hist {
            hist.record(nanos.clamp(Self::LOW_NS, Self::HIGH_NS))
                .unwrap();
        }
    }

    pub(crate) fn avg_nanos(&self) -> u64 {
        self.total_nanos.checked_div(self.count).unwrap_or(0)
    }

    pub(crate) fn percentile_nanos(&self, p: f64) -> u64 {
        match self.hist {
            Some(ref hist) if self.count > 0 => hist.value_at_percentile(p.clamp(0.0, 100.0)),
            _ => 0,
        }
    }
}

pub(crate) struct SqlInternalState {
    pub(crate) stats: HashMap<String, SqlEntry>,
}

pub(crate) struct SqlState {
    pub(crate) inner: Arc<StdRwLock<SqlInternalState>>,
    pub(crate) shutdown_tx: StdMutex<Option<CbSender<()>>>,
    pub(crate) completion_rx: StdMutex<Option<CbReceiver<()>>>,
}

pub(crate) static SQL_STATE: OnceLock<SqlState> = OnceLock::new();

pub(crate) fn get_sorted_sql_entries() -> Vec<SqlEntry> {
    let Some(state) = SQL_STATE.get() else {
        return Vec::new();
    };
    let guard = state.inner.read().unwrap();
    let mut stats: Vec<SqlEntry> = guard.stats.values().cloned().collect();
    stats.sort_by(compare_sql_entries);
    stats
}

pub(crate) fn get_sql_json() -> crate::json::JsonSqlList {
    let entries = get_sorted_sql_entries();
    let elapsed = std::time::Duration::from_nanos(crate::lib_on::current_elapsed_ns());
    let reference_total: u64 = entries.iter().map(|e| e.total_nanos).sum();
    crate::lib_on::report::collect_sql_json(
        &entries,
        elapsed,
        reference_total,
        &crate::lib_on::hotpath_guard::configured_percentiles(),
    )
}

static EVENT_QUEUES: EventQueueRegistry<SqlEvent> = EventQueueRegistry::new();

thread_local! {
    static EVENT_PRODUCER: EventProducer<SqlEvent> = EVENT_QUEUES.register();
}

#[inline]
pub(crate) fn send_sql_event(event: SqlEvent) {
    if !EVENT_QUEUES.is_active() {
        return;
    }
    let _suspend = crate::lib_on::SuspendAllocTracking::new();
    let _ = EVENT_PRODUCER.try_with(|producer| producer.push(event));
}

/// Stops producers ahead of the worker's final sweep at shutdown.
pub(crate) fn stop_sql_events() {
    EVENT_QUEUES.set_active(false);
}

fn process_sql_event(state: &mut SqlInternalState, event: SqlEvent) {
    let SqlEvent::Executed {
        sql,
        duration_nanos,
    } = event;

    let key = normalize::normalize(&sql);
    let entry = state
        .stats
        .entry(key.clone())
        .or_insert_with(|| SqlEntry::new(next_sql_id(), key));
    entry.count += 1;
    entry.total_nanos += duration_nanos;
    entry.record(duration_nanos);
}

fn flush_sql_buffer(buffer: &mut Vec<SqlEvent>, inner: &Arc<StdRwLock<SqlInternalState>>) {
    if buffer.is_empty() {
        return;
    }
    if let Ok(mut shared) = inner.write() {
        for e in buffer.drain(..) {
            process_sql_event(&mut shared, e);
        }
    }
}

/// Initialize the SQL statistics collection system (called on first emitted event).
pub(crate) fn init_sql_state() -> &'static SqlState {
    SQL_STATE.get_or_init(|| {
        START_TIME.get_or_init(Instant::now);

        let (shutdown_tx, shutdown_rx) = bounded::<()>(1);
        let (completion_tx, completion_rx) = bounded::<()>(1);

        let inner = Arc::new(StdRwLock::new(SqlInternalState {
            stats: HashMap::new(),
        }));
        let inner_clone = Arc::clone(&inner);

        EVENT_QUEUES.set_active(true);

        std::thread::Builder::new()
            .name("hp-sql".into())
            .spawn(move || {
                let flush_interval = std::time::Duration::from_millis(*DRAIN_INTERVAL_MS);
                let mut swept: Vec<SqlEvent> = Vec::new();

                // Single consumer of the per-thread event queues: capped sweep on
                // every tick, then one uncapped drain when shutdown is signalled
                // (producers are already stopped by then, so nothing is left behind).
                loop {
                    let shutdown = !matches!(
                        shutdown_rx.recv_timeout(flush_interval),
                        Err(RecvTimeoutError::Timeout)
                    );

                    if shutdown {
                        EVENT_QUEUES.drain_all(&mut swept);
                        flush_sql_buffer(&mut swept, &inner_clone);
                        break;
                    }

                    EVENT_QUEUES.sweep(&mut swept);
                    flush_sql_buffer(&mut swept, &inner_clone);
                }

                let _ = completion_tx.send(());
            })
            .expect("Failed to spawn sql-stats-collector thread");

        crate::metrics_server::start_metrics_server_once(*METRICS_SERVER_PORT);

        SqlState {
            inner,
            shutdown_tx: StdMutex::new(Some(shutdown_tx)),
            completion_rx: StdMutex::new(Some(completion_rx)),
        }
    })
}

/// Sort entries by total time spent (slowest aggregate first), tiebreak by count.
pub(crate) fn compare_sql_entries(a: &SqlEntry, b: &SqlEntry) -> std::cmp::Ordering {
    b.total_nanos
        .cmp(&a.total_nanos)
        .then_with(|| b.count.cmp(&a.count))
        .then_with(|| a.id.cmp(&b.id))
}
