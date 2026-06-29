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

use crossbeam_channel::{
    bounded, unbounded, Receiver as CbReceiver, Select, Sender as CbSender, TryRecvError,
};
use hdrhistogram::Histogram;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex as StdMutex, OnceLock, RwLock as StdRwLock};

use crate::batch::{register_thread_batch, BatchRegistry, BatchedMeasurement, MeasurementBatch};
use crate::instant::Instant;
use crate::lib_on::hotpath_guard::{
    WORKER_BATCH_SIZE, WORKER_FLUSH_INTERVAL_MS, WORKER_SHUTDOWN_DRAIN_LIMIT,
};
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
    Executed {
        sql: Arc<str>,
        duration_nanos: u64,
        elapsed_ns: u64,
    },
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

// `wrap = true` endpoint wrapper under `hotpath-meta` (instrumented), plain sender otherwise.
#[cfg(feature = "hotpath-meta")]
pub(crate) type SqlEventTx = hotpath_meta::wrap::crossbeam::Sender<Vec<SqlEvent>>;
#[cfg(not(feature = "hotpath-meta"))]
pub(crate) type SqlEventTx = CbSender<Vec<SqlEvent>>;

pub(crate) struct SqlState {
    pub(crate) event_tx: SqlEventTx,
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

static EVENT_REGISTRY: BatchRegistry<SqlEvent> = BatchRegistry::new();

thread_local! {
    static EVENT_BATCH: std::sync::Arc<std::sync::Mutex<MeasurementBatch<SqlEvent>>> =
        register_thread_batch(&EVENT_REGISTRY);
}

#[inline]
pub(crate) fn send_sql_event(event: SqlEvent) {
    let _suspend = crate::lib_on::SuspendAllocTracking::new();
    EVENT_BATCH.with(|b| {
        if let Ok(mut b) = b.lock() {
            b.add(event);
        }
    });
}

/// Flushes every thread's buffered SQL events into the worker channel.
/// Called at shutdown before the worker is signalled to stop.
pub(crate) fn flush_sql_batch() {
    EVENT_REGISTRY.flush_all();
}

impl BatchedMeasurement for SqlEvent {
    type Tx = SqlEventTx;

    fn elapsed_since_start_ns(&self) -> u64 {
        match self {
            SqlEvent::Executed { elapsed_ns, .. } => *elapsed_ns,
        }
    }

    fn fetch_sender() -> Option<Self::Tx> {
        Some(SQL_STATE.get()?.event_tx.clone())
    }

    fn send_batch(tx: &Self::Tx, batch: Vec<Self>) {
        let _ = tx.send(batch);
    }

    fn is_flush_boundary(&self) -> bool {
        false
    }
}

fn process_sql_event(state: &mut SqlInternalState, event: SqlEvent) {
    let SqlEvent::Executed {
        sql,
        duration_nanos,
        elapsed_ns: _,
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

        let (event_tx, event_rx) = unbounded::<Vec<SqlEvent>>();
        #[cfg(feature = "hotpath-meta")]
        let (event_tx, event_rx) =
            hotpath_meta::channel!((event_tx, event_rx), wrap = true, label = "hp-sql-events");
        let (shutdown_tx, shutdown_rx) = bounded::<()>(1);
        let (completion_tx, completion_rx) = bounded::<()>(1);

        let inner = Arc::new(StdRwLock::new(SqlInternalState {
            stats: HashMap::new(),
        }));
        let inner_clone = Arc::clone(&inner);

        std::thread::Builder::new()
            .name("hp-sql".into())
            .spawn(move || {
                let mut local_buffer: Vec<SqlEvent> = Vec::with_capacity(WORKER_BATCH_SIZE);
                let flush_interval = std::time::Duration::from_millis(WORKER_FLUSH_INTERVAL_MS);

                // Shutdown is checked before events; the `ready_timeout` tick flushes a partial buffer.
                let mut select = Select::new();
                let _shutdown_idx = select.recv(&shutdown_rx);
                #[cfg(feature = "hotpath-meta")]
                let _event_idx = select.recv(event_rx.select_handle());
                #[cfg(not(feature = "hotpath-meta"))]
                let _event_idx = select.recv(&event_rx);

                loop {
                    if select.ready_timeout(flush_interval).is_err() {
                        flush_sql_buffer(&mut local_buffer, &inner_clone);
                        continue;
                    }

                    if !matches!(shutdown_rx.try_recv(), Err(TryRecvError::Empty)) {
                        for _ in 0..WORKER_SHUTDOWN_DRAIN_LIMIT {
                            match event_rx.try_recv() {
                                Ok(events) => local_buffer.extend(events),
                                Err(_) => break,
                            }
                        }
                        flush_sql_buffer(&mut local_buffer, &inner_clone);
                        break;
                    }

                    match event_rx.try_recv() {
                        Ok(events) => {
                            local_buffer.extend(events);
                            if local_buffer.len() >= WORKER_BATCH_SIZE {
                                flush_sql_buffer(&mut local_buffer, &inner_clone);
                            }
                        }
                        // A disconnected receiver stays ready; flush and stop, do not spin.
                        Err(TryRecvError::Disconnected) => {
                            flush_sql_buffer(&mut local_buffer, &inner_clone);
                            break;
                        }
                        Err(TryRecvError::Empty) => {}
                    }
                }

                let _ = completion_tx.send(());
            })
            .expect("Failed to spawn sql-stats-collector thread");

        crate::metrics_server::start_metrics_server_once(*METRICS_SERVER_PORT);

        SqlState {
            event_tx,
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
