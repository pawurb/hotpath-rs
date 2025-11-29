//! Futures instrumentation module - prints lifecycle events for debugging.

use crossbeam_channel::{unbounded, Sender as CbSender};
use std::sync::atomic::AtomicU64;
use std::sync::OnceLock;

pub(crate) mod wrapper;

pub use wrapper::{InstrumentedFuture, InstrumentedFutureLog};

pub(crate) static FUTURE_ID_COUNTER: AtomicU64 = AtomicU64::new(0);

type FuturesState = CbSender<FutureEvent>;

static FUTURES_STATE: OnceLock<FuturesState> = OnceLock::new();

/// Initialize the futures event collection system (called on first instrumented future).
pub fn init_futures_state() {
    FUTURES_STATE.get_or_init(|| {
        let (tx, rx) = unbounded::<FutureEvent>();

        std::thread::Builder::new()
            .name("hp-futures".into())
            .spawn(move || {
                while let Ok(event) = rx.recv() {
                    dbg!(&event);
                }
            })
            .expect("Failed to spawn futures event collector thread");

        tx
    });
}

/// Send a future event to the background thread.
pub(crate) fn send_future_event(event: FutureEvent) {
    if let Some(tx) = FUTURES_STATE.get() {
        let _ = tx.send(event);
    }
}

/// Result of polling a future.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PollResult {
    Pending,
    Ready(Option<String>),
}

/// Events emitted during the lifecycle of an instrumented future.
#[derive(Debug, Clone)]
pub(crate) enum FutureEvent {
    /// A new instrumented future was created.
    Created {
        /// Unique identifier for this future instance.
        id: u64,
        /// Source location where the future was created (file:line).
        source: &'static str,
    },
    /// The future was polled.
    Polled {
        /// Unique identifier for this future instance.
        id: u64,
        /// The poll count (1-indexed).
        poll_count: usize,
        /// The result of the poll.
        result: PollResult,
    },
    /// The future's waker was invoked.
    Wake {
        /// Unique identifier for this future instance.
        id: u64,
    },
    /// The future was dropped.
    Dropped {
        /// Unique identifier for this future instance.
        id: u64,
    },
}

/// Trait for instrumenting futures (no Debug requirement).
///
/// This trait is not intended for direct use. Use the `future!` macro instead.
#[doc(hidden)]
pub trait InstrumentFuture {
    type Output;
    fn instrument_future(self, source: &'static str) -> Self::Output;
}

/// Trait for instrumenting futures with output logging (requires Debug).
///
/// This trait is not intended for direct use. Use the `future!` macro with `log = true` instead.
#[doc(hidden)]
pub trait InstrumentFutureLog {
    type Output;
    fn instrument_future_log(self, source: &'static str) -> Self::Output;
}

impl<F: std::future::Future> InstrumentFuture for F {
    type Output = InstrumentedFuture<F>;

    fn instrument_future(self, source: &'static str) -> Self::Output {
        InstrumentedFuture::new(self, source)
    }
}

impl<F: std::future::Future> InstrumentFutureLog for F
where
    F::Output: std::fmt::Debug,
{
    type Output = InstrumentedFutureLog<F>;

    fn instrument_future_log(self, source: &'static str) -> Self::Output {
        InstrumentedFutureLog::new(self, source)
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
        $crate::futures::init_futures_state();
        $crate::InstrumentFuture::instrument_future($fut, TASK_LOC)
    }};

    // With logging: requires Debug
    ($fut:expr, log = true) => {{
        const TASK_LOC: &'static str = concat!(file!(), ":", line!());
        $crate::futures::init_futures_state();
        $crate::InstrumentFutureLog::instrument_future_log($fut, TASK_LOC)
    }};
}
