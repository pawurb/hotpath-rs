//! Tasks instrumentation module - prints lifecycle events for debugging.

use crossbeam_channel::{unbounded, Sender as CbSender};
use std::sync::atomic::AtomicU64;
use std::sync::OnceLock;

pub(crate) mod wrapper;

pub use wrapper::{InstrumentedTask, InstrumentedTaskLog};

pub(crate) static TASK_ID_COUNTER: AtomicU64 = AtomicU64::new(0);

type TasksState = CbSender<TaskEvent>;

static TASKS_STATE: OnceLock<TasksState> = OnceLock::new();

/// Initialize the tasks event collection system (called on first instrumented future).
pub fn init_tasks_state() {
    TASKS_STATE.get_or_init(|| {
        let (tx, rx) = unbounded::<TaskEvent>();

        std::thread::Builder::new()
            .name("hp-tasks".into())
            .spawn(move || {
                while let Ok(event) = rx.recv() {
                    dbg!(&event);
                }
            })
            .expect("Failed to spawn tasks event collector thread");

        tx
    });
}

/// Send a task event to the background thread.
pub(crate) fn send_task_event(event: TaskEvent) {
    if let Some(tx) = TASKS_STATE.get() {
        let _ = tx.send(event);
    }
}

/// Result of polling a future.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PollResult {
    Pending,
    Ready(Option<String>),
}

/// Events emitted during the lifecycle of an instrumented task (future).
#[derive(Debug, Clone)]
pub(crate) enum TaskEvent {
    /// A new instrumented task was created.
    Created {
        /// Unique identifier for this task instance.
        id: u64,
        /// Source location where the task was created (file:line).
        source: &'static str,
    },
    /// The task was polled.
    Polled {
        /// Unique identifier for this task instance.
        id: u64,
        /// The poll count (1-indexed).
        poll_count: usize,
        /// The result of the poll.
        result: PollResult,
    },
    /// The task's waker was invoked.
    Wake {
        /// Unique identifier for this task instance.
        id: u64,
    },
    /// The task was dropped.
    Dropped {
        /// Unique identifier for this task instance.
        id: u64,
    },
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
