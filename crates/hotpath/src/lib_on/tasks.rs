//! Tasks instrumentation module - tracks async Future lifecycle and poll statistics.

use crate::channels::{get_log_limit, resolve_label, LogEntry, START_TIME};
use crossbeam_channel::{unbounded, Sender as CbSender};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, OnceLock, RwLock};

#[cfg(target_os = "linux")]
use quanta::Instant;

#[cfg(not(target_os = "linux"))]
use std::time::Instant;

pub mod guard;
pub(crate) mod wrapper;

pub use guard::{FuturesGuard, FuturesGuardBuilder};
pub use wrapper::{InstrumentedTask, InstrumentedTaskLog};

pub(crate) static TASK_ID_COUNTER: AtomicU64 = AtomicU64::new(0);

/// State of an instrumented task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TaskState {
    #[default]
    Pending,
    Running,
    Suspended,
    Ready,
    Cancelled,
}

impl std::fmt::Display for TaskState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl TaskState {
    pub fn as_str(&self) -> &'static str {
        match self {
            TaskState::Pending => "pending",
            TaskState::Running => "running",
            TaskState::Suspended => "suspended",
            TaskState::Ready => "ready",
            TaskState::Cancelled => "cancelled",
        }
    }
}

impl Serialize for TaskState {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for TaskState {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        match s.as_str() {
            "pending" => Ok(TaskState::Pending),
            "running" => Ok(TaskState::Running),
            "suspended" => Ok(TaskState::Suspended),
            "ready" => Ok(TaskState::Ready),
            "cancelled" => Ok(TaskState::Cancelled),
            _ => Err(serde::de::Error::custom("invalid task state")),
        }
    }
}

/// Statistics for a single instrumented task.
#[derive(Debug, Clone)]
pub struct TaskStats {
    pub id: u64,
    pub source: &'static str,
    pub label: Option<String>,
    pub iter: u32,
    pub state: TaskState,
    pub poll_count: u64,
    pub result: Option<String>,
    pub poll_logs: VecDeque<LogEntry>,
}

impl TaskStats {
    fn new(id: u64, source: &'static str, label: Option<String>, iter: u32) -> Self {
        Self {
            id,
            source,
            label,
            iter,
            state: TaskState::default(),
            poll_count: 0,
            result: None,
            poll_logs: VecDeque::new(),
        }
    }

    /// Whether task is still active (not completed/cancelled)
    pub fn is_active(&self) -> bool {
        matches!(
            self.state,
            TaskState::Pending | TaskState::Running | TaskState::Suspended
        )
    }

    /// Get the result, either from the dedicated field or from the last poll log
    pub fn get_result(&self) -> Option<&str> {
        if let Some(ref result) = self.result {
            return Some(result.as_str());
        }
        // Try to get from last poll log if state is completed
        if self.state == TaskState::Ready {
            if let Some(last_log) = self.poll_logs.back() {
                return last_log.message.as_deref();
            }
        }
        None
    }
}

/// Wrapper for tasks-only JSON response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TasksJson {
    pub current_elapsed_ns: u64,
    pub tasks: Vec<SerializableTaskStats>,
}

/// Serializable version of task statistics for JSON responses.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializableTaskStats {
    pub id: u64,
    pub source: String,
    pub label: String,
    pub has_custom_label: bool,
    pub state: TaskState,
    pub iter: u32,
    pub poll_count: u64,
    pub result: Option<String>,
}

impl From<&TaskStats> for SerializableTaskStats {
    fn from(task_stats: &TaskStats) -> Self {
        let label = resolve_label(
            task_stats.source,
            task_stats.label.as_deref(),
            task_stats.iter,
        );

        Self {
            id: task_stats.id,
            source: task_stats.source.to_string(),
            label,
            has_custom_label: task_stats.label.is_some(),
            state: task_stats.state,
            iter: task_stats.iter,
            poll_count: task_stats.poll_count,
            result: task_stats.get_result().map(|s| s.to_string()),
        }
    }
}

/// Serializable log response for task poll logs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskLogs {
    pub id: String,
    pub poll_logs: Vec<LogEntry>,
}

/// Result of polling a future.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PollResult {
    Pending,
    Ready,
}

/// Events emitted during the lifecycle of an instrumented task (future).
#[derive(Debug)]
pub(crate) enum TaskEvent {
    Created {
        id: u64,
        source: &'static str,
        display_label: Option<String>,
    },
    Polled {
        id: u64,
        timestamp: Instant,
        tid: u64,
        result: PollResult,
        log_message: Option<String>,
    },
    Completed {
        id: u64,
    },
    Cancelled {
        id: u64,
    },
}

type TasksState = (CbSender<TaskEvent>, Arc<RwLock<HashMap<u64, TaskStats>>>);

static TASKS_STATE: OnceLock<TasksState> = OnceLock::new();

/// Initialize the tasks event collection system (called on first instrumented future).
#[doc(hidden)]
pub fn init_tasks_state() {
    TASKS_STATE.get_or_init(|| {
        START_TIME.get_or_init(Instant::now);

        let (tx, rx) = unbounded::<TaskEvent>();
        let stats_map = Arc::new(RwLock::new(HashMap::<u64, TaskStats>::new()));
        let stats_map_clone = Arc::clone(&stats_map);

        std::thread::Builder::new()
            .name("hp-tasks".into())
            .spawn(move || {
                while let Ok(event) = rx.recv() {
                    let mut stats = stats_map_clone.write().unwrap();
                    match event {
                        TaskEvent::Created {
                            id,
                            source,
                            display_label,
                        } => {
                            let iter = stats.values().filter(|s| s.source == source).count() as u32;

                            stats.insert(id, TaskStats::new(id, source, display_label, iter));
                        }
                        TaskEvent::Polled {
                            id,
                            timestamp,
                            tid,
                            result,
                            log_message,
                        } => {
                            if let Some(task_stats) = stats.get_mut(&id) {
                                task_stats.poll_count += 1;

                                // Update state and capture result based on poll result
                                match result {
                                    PollResult::Pending => {
                                        task_stats.state = TaskState::Suspended;
                                    }
                                    PollResult::Ready => {
                                        task_stats.state = TaskState::Ready;
                                        // Capture the result if available
                                        if log_message.is_some() {
                                            task_stats.result = log_message.clone();
                                        }
                                    }
                                };

                                let limit = get_log_limit();
                                if task_stats.poll_logs.len() >= limit {
                                    task_stats.poll_logs.pop_front();
                                }
                                task_stats.poll_logs.push_back(LogEntry::new(
                                    task_stats.poll_count,
                                    timestamp,
                                    log_message,
                                    Some(tid),
                                ));
                            }
                        }
                        TaskEvent::Completed { id } => {
                            if let Some(task_stats) = stats.get_mut(&id) {
                                task_stats.state = TaskState::Ready;
                            }
                        }
                        TaskEvent::Cancelled { id } => {
                            if let Some(task_stats) = stats.get_mut(&id) {
                                if task_stats.state != TaskState::Ready {
                                    task_stats.state = TaskState::Cancelled;
                                }
                            }
                        }
                    }
                }
            })
            .expect("Failed to spawn tasks event collector thread");

        (tx, stats_map)
    });
}

/// Send a task event to the background thread.
pub(crate) fn send_task_event(event: TaskEvent) {
    if let Some((tx, _)) = TASKS_STATE.get() {
        let _ = tx.send(event);
    }
}

/// Trait for instrumenting tasks (no Debug requirement).
///
/// This trait is not intended for direct use. Use the `future!` macro instead.
#[doc(hidden)]
pub trait InstrumentTask {
    type Output;
    fn instrument_task(self, source: &'static str) -> Self::Output;
}

/// Trait for instrumenting tasks with output logging (requires Debug).
///
/// This trait is not intended for direct use. Use the `future!` macro with `log = true` instead.
#[doc(hidden)]
pub trait InstrumentTaskLog {
    type Output;
    fn instrument_task_log(self, source: &'static str) -> Self::Output;
}

impl<F: std::future::Future> InstrumentTask for F {
    type Output = InstrumentedTask<F>;

    fn instrument_task(self, source: &'static str) -> Self::Output {
        InstrumentedTask::new(self, source)
    }
}

impl<F: std::future::Future> InstrumentTaskLog for F
where
    F::Output: std::fmt::Debug,
{
    type Output = InstrumentedTaskLog<F>;

    fn instrument_task_log(self, source: &'static str) -> Self::Output {
        InstrumentedTaskLog::new(self, source)
    }
}

fn get_all_task_stats() -> HashMap<u64, TaskStats> {
    if let Some((_, stats_map)) = TASKS_STATE.get() {
        stats_map.read().unwrap().clone()
    } else {
        HashMap::new()
    }
}

/// Compare two task stats for sorting.
/// Custom labels come first (sorted alphabetically), then auto-generated labels (sorted by source and iter).
fn compare_task_stats(a: &TaskStats, b: &TaskStats) -> std::cmp::Ordering {
    let a_has_label = a.label.is_some();
    let b_has_label = b.label.is_some();

    match (a_has_label, b_has_label) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        (true, true) => a
            .label
            .as_ref()
            .unwrap()
            .cmp(b.label.as_ref().unwrap())
            .then_with(|| a.iter.cmp(&b.iter)),
        (false, false) => a.source.cmp(b.source).then_with(|| a.iter.cmp(&b.iter)),
    }
}

pub fn get_sorted_task_stats() -> Vec<TaskStats> {
    let mut stats: Vec<TaskStats> = get_all_task_stats().into_values().collect();
    stats.sort_by(compare_task_stats);
    stats
}

pub fn get_tasks_json() -> TasksJson {
    let tasks = get_sorted_task_stats()
        .iter()
        .map(SerializableTaskStats::from)
        .collect();

    let current_elapsed_ns = START_TIME
        .get()
        .map(|t| t.elapsed().as_nanos() as u64)
        .unwrap_or(0);

    TasksJson {
        current_elapsed_ns,
        tasks,
    }
}

pub fn get_task_logs(task_id: &str) -> Option<TaskLogs> {
    let id = task_id.parse::<u64>().ok()?;
    let stats = get_all_task_stats();
    stats.get(&id).map(|task_stats| {
        let mut poll_logs: Vec<LogEntry> = task_stats.poll_logs.iter().cloned().collect();

        // Sort by index descending (most recent first)
        poll_logs.sort_by(|a, b| b.index.cmp(&a.index));

        TaskLogs {
            id: task_id.to_string(),
            poll_logs,
        }
    })
}

/// Instrument a future to inspect future's lifecycle events.
///
/// # Variants
///
/// - `future!(expr)` - No Debug requirement, prints `Ready` without the value
/// - `future!(expr, log = true)` - Requires Debug, prints `Ready(value)`
///
/// # Examples
///
/// ```rust,ignore
/// use hotpath::future;
///
/// // Without logging (works with any output type)
/// let result = future!(async { NoDebugType::new() }).await;
///
/// // With logging (requires Debug on output type)
/// let result = future!(async { 42 }, log = true).await;
/// ```
#[macro_export]
macro_rules! future {
    // Basic: no Debug requirement
    ($fut:expr) => {{
        const TASK_LOC: &'static str = concat!(file!(), ":", line!());
        $crate::tasks::init_tasks_state();
        $crate::InstrumentTask::instrument_task($fut, TASK_LOC)
    }};

    // With logging: requires Debug
    ($fut:expr, log = true) => {{
        const TASK_LOC: &'static str = concat!(file!(), ":", line!());
        $crate::tasks::init_tasks_state();
        $crate::InstrumentTaskLog::instrument_task_log($fut, TASK_LOC)
    }};
}
