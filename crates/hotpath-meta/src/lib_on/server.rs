//! HTTP server instrumentation module - tracks response times per matched
//! route (`GET /users/{id}`).
//!
//! Entries are keyed by `METHOD route-template`, so the router's own path
//! template does the bucketing; requests that never matched a route
//! (fallbacks, `nest_service` targets) fall back to the raw path normalized
//! by [`crate::lib_on::http::normalize`]. Distinct keys are capped at
//! `HOTPATH_META_ENTRIES_LIMIT`; beyond that new keys land in the `<other>` bucket
//! (see [`crate::lib_on::hotpath_guard::bounded_key`]).
//!
//! The write path (worker, events) is driven by the tower [`AxumLayer`]
//! (attached via the `axum!` macro or `Router::layer`), gated behind the
//! `axum-0-8` feature; the read path stays compiled so the report/metrics
//! wiring is feature-uniform.
#![cfg_attr(not(feature = "axum-0-8"), allow(dead_code))]

use crossbeam_channel::{bounded, Receiver as CbReceiver, RecvTimeoutError, Sender as CbSender};
use hdrhistogram::Histogram;
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex as StdMutex, OnceLock, RwLock as StdRwLock};

use crate::batch::{EventProducer, EventQueueRegistry};
use crate::instant::Instant;
use crate::json::{HttpLogEntry, HttpLogs};
use crate::lib_on::hotpath_guard::{bounded_key, DRAIN_INTERVAL_MS, LOGS_LIMIT, OVERFLOW_ENTRY};
use crate::lib_on::http::normalize::normalize_endpoint;
use crate::lib_on::START_TIME;
use crate::metrics_server::METRICS_SERVER_PORT;

#[cfg(feature = "axum-0-8")]
mod axum_08;

static SERVER_ID_COUNTER: AtomicU32 = AtomicU32::new(1);

fn next_server_id() -> u32 {
    SERVER_ID_COUNTER.fetch_add(1, Ordering::Relaxed)
}

/// Tower layer that reports per-request response times to the hotpath server
/// worker, bucketed by the axum route template that handled the request.
///
/// The `axum!` macro appends it automatically; apps that build their own
/// middleware stack can add it with `Router::layer(hotpath_meta::AxumLayer::new())`.
/// `Router::layer` only wraps routes added before it, so attach it after the
/// last `.route(..)` / `.fallback(..)` call.
#[derive(Clone, Copy, Debug)]
pub struct AxumLayer {
    _private: (),
}

impl AxumLayer {
    pub fn new() -> Self {
        init_server_state();
        Self { _private: () }
    }
}

impl Default for AxumLayer {
    fn default() -> Self {
        Self::new()
    }
}

/// Events sent to the background server statistics collection thread.
#[derive(Debug)]
pub(crate) enum ServerEvent {
    /// Emitted when the response head is produced for a request. `route` is
    /// `METHOD template` when the router matched a route; otherwise it is
    /// `METHOD raw-path` and `matched` is `false`, which makes the worker
    /// normalize id-like segments before bucketing. `timestamp_ns` is the
    /// completion time in ns since profiler start.
    Completed {
        route: Arc<str>,
        matched: bool,
        duration_nanos: u64,
        status: u16,
        timestamp_ns: u64,
    },
}

/// Aggregated statistics for a single route.
#[derive(Debug, Clone)]
pub(crate) struct ServerEntry {
    pub(crate) id: u32,
    pub(crate) route: String,
    pub(crate) count: u64,
    /// Responses with a 4xx status.
    pub(crate) status_4xx: u64,
    /// Responses with a 5xx status.
    pub(crate) status_5xx: u64,
    pub(crate) total_nanos: u64,
    hist: Option<Histogram<u64>>,
}

impl ServerEntry {
    const LOW_NS: u64 = 1;
    const HIGH_NS: u64 = 1_000_000_000_000; // 1000s
    const SIGFIGS: u8 = 3;

    fn new(id: u32, route: String) -> Self {
        Self {
            id,
            route,
            count: 0,
            status_4xx: 0,
            status_5xx: 0,
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

pub(crate) struct ServerInternalState {
    pub(crate) stats: HashMap<String, ServerEntry>,
    /// Recent requests per entry id, capped at `LOGS_LIMIT`. Only status and
    /// timing are kept - raw request paths are never stored.
    pub(crate) logs: HashMap<u32, VecDeque<HttpLogEntry>>,
}

pub(crate) struct ServerState {
    pub(crate) inner: Arc<StdRwLock<ServerInternalState>>,
    pub(crate) shutdown_tx: StdMutex<Option<CbSender<()>>>,
    pub(crate) completion_rx: StdMutex<Option<CbReceiver<()>>>,
}

pub(crate) static SERVER_STATE: OnceLock<ServerState> = OnceLock::new();

pub(crate) fn get_sorted_server_entries() -> Vec<ServerEntry> {
    let Some(state) = SERVER_STATE.get() else {
        return Vec::new();
    };
    let guard = state.inner.read().unwrap();
    let mut stats: Vec<ServerEntry> = guard.stats.values().cloned().collect();
    stats.sort_by(compare_server_entries);
    stats
}

/// Returns recent requests of the route entry with the given id, newest first.
pub(crate) fn get_server_logs(id: u32) -> Option<HttpLogs> {
    let state = SERVER_STATE.get()?;
    let guard = state.inner.read().unwrap();
    let logs = guard.logs.get(&id)?;
    Some(HttpLogs {
        id,
        logs: logs.iter().rev().cloned().collect(),
    })
}

pub(crate) fn get_server_json() -> crate::json::JsonServerList {
    let entries = get_sorted_server_entries();
    let elapsed = std::time::Duration::from_nanos(crate::lib_on::current_elapsed_ns());
    let reference_total: u64 = entries.iter().map(|e| e.total_nanos).sum();
    let total_calls: u64 = entries.iter().map(|e| e.count).sum();
    crate::lib_on::report::collect_server_json(
        &entries,
        elapsed,
        total_calls,
        reference_total,
        &crate::lib_on::hotpath_guard::configured_percentiles(),
    )
}

static EVENT_QUEUES: EventQueueRegistry<ServerEvent> = EventQueueRegistry::new();

thread_local! {
    static EVENT_PRODUCER: EventProducer<ServerEvent> = EVENT_QUEUES.register();
}

#[inline]
pub(crate) fn send_server_event(event: ServerEvent) {
    if !EVENT_QUEUES.is_active() {
        return;
    }
    let _suspend = crate::lib_on::SuspendAllocTracking::new();
    let _ = EVENT_PRODUCER.try_with(|producer| producer.push(event));
}

/// Stops producers ahead of the worker's final sweep at shutdown.
pub(crate) fn stop_server_events() {
    EVENT_QUEUES.set_active(false);
    #[cfg(feature = "axum-0-8")]
    crate::lib_on::caller_stack::set_route_scope(false);
}

fn process_server_event(state: &mut ServerInternalState, event: ServerEvent) {
    let ServerEvent::Completed {
        route,
        matched,
        duration_nanos,
        status,
        timestamp_ns,
    } = event;

    let key = if matched {
        route.to_string()
    } else {
        normalize_endpoint(&route)
    };
    let key = bounded_key(&state.stats, key, || OVERFLOW_ENTRY.to_string());
    let entry = state
        .stats
        .entry(key)
        .or_insert_with_key(|route| ServerEntry::new(next_server_id(), route.clone()));
    entry.count += 1;
    entry.total_nanos += duration_nanos;
    match status {
        400..=499 => entry.status_4xx += 1,
        500..=599 => entry.status_5xx += 1,
        _ => {}
    }
    entry.record(duration_nanos);

    let logs = state.logs.entry(entry.id).or_default();
    if logs.len() >= *LOGS_LIMIT {
        logs.pop_front();
    }
    logs.push_back(HttpLogEntry {
        index: entry.count,
        timestamp: timestamp_ns,
        duration_nanos,
        status: Some(status),
    });
}

fn flush_server_buffer(buffer: &mut Vec<ServerEvent>, inner: &Arc<StdRwLock<ServerInternalState>>) {
    if buffer.is_empty() {
        return;
    }
    if let Ok(mut shared) = inner.write() {
        for e in buffer.drain(..) {
            process_server_event(&mut shared, e);
        }
    }
}

/// Initialize the server statistics collection system (called by the `axum!`
/// macro and the [`AxumLayer`] constructor).
pub fn init_server_state() {
    SERVER_STATE.get_or_init(|| {
        START_TIME.get_or_init(Instant::now);

        let (shutdown_tx, shutdown_rx) = bounded::<()>(1);
        let (completion_tx, completion_rx) = bounded::<()>(1);

        let inner = Arc::new(StdRwLock::new(ServerInternalState {
            stats: HashMap::new(),
            logs: HashMap::new(),
        }));
        let inner_clone = Arc::clone(&inner);

        EVENT_QUEUES.set_active(true);

        std::thread::Builder::new()
            .name("hp-server".into())
            .spawn(move || {
                let flush_interval = std::time::Duration::from_millis(*DRAIN_INTERVAL_MS);
                let mut swept: Vec<ServerEvent> = Vec::new();

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
                        flush_server_buffer(&mut swept, &inner_clone);
                        break;
                    }

                    EVENT_QUEUES.sweep(&mut swept);
                    flush_server_buffer(&mut swept, &inner_clone);
                }

                let _ = completion_tx.send(());
            })
            .expect("Failed to spawn server-stats-collector thread");

        crate::metrics_server::start_metrics_server_once(*METRICS_SERVER_PORT);

        ServerState {
            inner,
            shutdown_tx: StdMutex::new(Some(shutdown_tx)),
            completion_rx: StdMutex::new(Some(completion_rx)),
        }
    });
}

/// Sort entries by total time spent (slowest aggregate first), tiebreak by count.
pub(crate) fn compare_server_entries(a: &ServerEntry, b: &ServerEntry) -> std::cmp::Ordering {
    b.total_nanos
        .cmp(&a.total_nanos)
        .then_with(|| b.count.cmp(&a.count))
        .then_with(|| a.id.cmp(&b.id))
}

/// Wrap an axum `Router` so every request it serves is timed and reported in
/// the `server` section, keyed by matched route (`GET /users/{id}`).
///
/// Expands to `router.layer(hotpath_meta::AxumLayer::new())`, so it must be applied
/// after the last `.route(..)` / `.fallback(..)` call - `Router::layer` only
/// wraps routes that already exist. With the `hotpath-meta` feature disabled the
/// router is returned unchanged.
///
/// # Examples
///
/// ```rust,ignore
/// let app = hotpath_meta::axum!(Router::new()
///     .route("/users/{id}", get(get_user))
///     .route("/users", post(create_user)));
///
/// axum::serve(listener, app).await?;
/// ```
#[macro_export]
macro_rules! axum {
    ($router:expr) => {
        $router.layer($crate::AxumLayer::new())
    };
}
