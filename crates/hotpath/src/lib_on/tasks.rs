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

/// A single invocation/call of a task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskCall {
    pub id: u64,
    pub task_id: u64,
    pub state: TaskState,
    pub poll_count: u64,
    pub result: Option<String>,
}

impl TaskCall {
    fn new(id: u64, task_id: u64) -> Self {
        Self {
            id,
            task_id,
            state: TaskState::default(),
            poll_count: 0,
            result: None,
        }
    }
}

/// Aggregated statistics for a source location.
#[derive(Debug, Clone)]
pub struct TaskStats {
    pub id: u64,
    pub source: &'static str,
    pub label: Option<String>,
    pub task_calls: Vec<TaskCall>,
    pub poll_logs: VecDeque<LogEntry>,
}

impl TaskStats {
    fn new(id: u64, source: &'static str, label: Option<String>) -> Self {
        Self {
            id,
            source,
            label,
            task_calls: Vec::new(),
            poll_logs: VecDeque::new(),
        }
    }

    /// Total number of invocations at this source location
    pub fn call_count(&self) -> usize {
        self.task_calls.len()
    }

    /// Total polls across all invocations
    pub fn total_polls(&self) -> u64 {
        self.task_calls.iter().map(|c| c.poll_count).sum()
    }

    /// Find a call by ID
    fn find_call_mut(&mut self, id: u64) -> Option<&mut TaskCall> {
        self.task_calls.iter_mut().find(|c| c.id == id)
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
    pub call_count: usize,
    pub total_polls: u64,
    pub task_calls: Vec<TaskCall>,
}

impl From<&TaskStats> for SerializableTaskStats {
    fn from(task_stats: &TaskStats) -> Self {
        let label = resolve_label(task_stats.source, task_stats.label.as_deref(), None);

        Self {
            id: task_stats.id,
            source: task_stats.source.to_string(),
            label,
            has_custom_label: task_stats.label.is_some(),
            call_count: task_stats.call_count(),
            total_polls: task_stats.total_polls(),
            task_calls: task_stats.task_calls.clone(),
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

pub(crate) type TasksState = (CbSender<TaskEvent>, Arc<RwLock<HashMap<u64, TaskStats>>>);

static TASKS_STATE: OnceLock<TasksState> = OnceLock::new();

/// Initialize the tasks event collection system (called on first instrumented future).
#[doc(hidden)]
pub fn init_tasks_state() {
    TASKS_STATE.get_or_init(|| {
        START_TIME.get_or_init(Instant::now);

        // Start HTTP server if HOTPATH_HTTP_PORT is set
        #[cfg(feature = "hotpath")]
        if let Ok(port_str) = std::env::var("HOTPATH_HTTP_PORT") {
            if let Ok(port) = port_str.parse::<u16>() {
                crate::http_server::start_metrics_server_once(port);
            }
        }

        let (tx, rx) = unbounded::<TaskEvent>();
        let stats_map = Arc::new(RwLock::new(HashMap::<u64, TaskStats>::new()));
        let stats_map_clone = Arc::clone(&stats_map);

        std::thread::Builder::new()
            .name("hp-tasks".into())
            .spawn(move || {
                // Thread-local lookup maps
                let mut source_to_id: HashMap<&'static str, u64> = HashMap::new();
                let mut call_to_task: HashMap<u64, u64> = HashMap::new();
                let mut next_task_id: u64 = 0;

                while let Ok(event) = rx.recv() {
                    let mut stats = stats_map_clone.write().unwrap();
                    match event {
                        TaskEvent::Created {
                            id,
                            source,
                            display_label,
                        } => {
                            // Get or create TaskStats for this source location
                            let task_id = *source_to_id.entry(source).or_insert_with(|| {
                                let task_id = next_task_id;
                                next_task_id += 1;
                                stats.insert(
                                    task_id,
                                    TaskStats::new(task_id, source, display_label),
                                );
                                task_id
                            });

                            // Add new call/invocation with foreign key to task
                            if let Some(task_stats) = stats.get_mut(&task_id) {
                                task_stats.task_calls.push(TaskCall::new(id, task_id));
                            }

                            // Track call id -> task id mapping for routing events
                            call_to_task.insert(id, task_id);
                        }
                        TaskEvent::Polled {
                            id,
                            timestamp,
                            tid,
                            result,
                            log_message,
                        } => {
                            // Look up task from call id
                            if let Some(&task_id) = call_to_task.get(&id) {
                                if let Some(task_stats) = stats.get_mut(&task_id) {
                                    if let Some(call) = task_stats.find_call_mut(id) {
                                        call.poll_count += 1;

                                        // Update state and capture result based on poll result
                                        match result {
                                            PollResult::Pending => {
                                                call.state = TaskState::Suspended;
                                            }
                                            PollResult::Ready => {
                                                call.state = TaskState::Ready;
                                                // Capture the result if available
                                                if log_message.is_some() {
                                                    call.result = log_message.clone();
                                                }
                                            }
                                        };
                                    }

                                    // Add to poll logs at the TaskStats level
                                    let total_polls = task_stats.total_polls();
                                    let limit = get_log_limit();
                                    if task_stats.poll_logs.len() >= limit {
                                        task_stats.poll_logs.pop_front();
                                    }
                                    task_stats.poll_logs.push_back(LogEntry::new(
                                        total_polls,
                                        timestamp,
                                        log_message,
                                        Some(tid),
                                    ));
                                }
                            }
                        }
                        TaskEvent::Completed { id } => {
                            if let Some(&task_id) = call_to_task.get(&id) {
                                if let Some(task_stats) = stats.get_mut(&task_id) {
                                    if let Some(call) = task_stats.find_call_mut(id) {
                                        call.state = TaskState::Ready;
                                    }
                                }
                            }
                        }
                        TaskEvent::Cancelled { id } => {
                            if let Some(&task_id) = call_to_task.get(&id) {
                                if let Some(task_stats) = stats.get_mut(&task_id) {
                                    if let Some(call) = task_stats.find_call_mut(id) {
                                        if call.state != TaskState::Ready {
                                            call.state = TaskState::Cancelled;
                                        }
                                    }
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
/// Custom labels come first (sorted alphabetically), then auto-generated labels (sorted by source).
fn compare_task_stats(a: &TaskStats, b: &TaskStats) -> std::cmp::Ordering {
    let a_has_label = a.label.is_some();
    let b_has_label = b.label.is_some();

    match (a_has_label, b_has_label) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        (true, true) => a.label.as_ref().unwrap().cmp(b.label.as_ref().unwrap()),
        (false, false) => a.source.cmp(b.source),
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

pub fn get_task_logs(task_id: u64) -> Option<TaskLogs> {
    let stats = get_all_task_stats();
    // Look up TaskStats directly by task_id
    stats.get(&task_id).map(|task_stats| {
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
