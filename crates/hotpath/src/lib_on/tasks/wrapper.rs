//! Instrumented Task wrapper that tracks lifecycle events.

use super::{get_or_create_task_id, send_task_event, PollResult, TaskEvent, TASK_CALL_ID_COUNTER};
use pin_project_lite::pin_project;
use std::future::Future;
use std::mem::ManuallyDrop;
use std::pin::Pin;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

#[cfg(target_os = "linux")]
use quanta::Instant;

#[cfg(not(target_os = "linux"))]
use std::time::Instant;

struct WakerData {
    inner: Waker,
}

fn waker_clone(data: *const ()) -> RawWaker {
    let arc = ManuallyDrop::new(unsafe { Arc::from_raw(data as *const WakerData) });
    let cloned = Arc::clone(&arc);
    RawWaker::new(Arc::into_raw(cloned) as *const (), &VTABLE)
}

fn waker_wake(data: *const ()) {
    unsafe { Arc::increment_strong_count(data as *const WakerData) };
    let arc = unsafe { Arc::from_raw(data as *const WakerData) };
    arc.inner.wake_by_ref();
}

fn waker_wake_by_ref(data: *const ()) {
    let arc = ManuallyDrop::new(unsafe { Arc::from_raw(data as *const WakerData) });
    arc.inner.wake_by_ref();
}

fn waker_drop(data: *const ()) {
    unsafe {
        Arc::from_raw(data as *const WakerData);
    }
}

static VTABLE: RawWakerVTable =
    RawWakerVTable::new(waker_clone, waker_wake, waker_wake_by_ref, waker_drop);

fn create_instrumented_waker(waker: &Waker) -> Waker {
    let data = Arc::new(WakerData {
        inner: waker.clone(),
    });
    let raw = RawWaker::new(Arc::into_raw(data) as *const (), &VTABLE);
    unsafe { Waker::from_raw(raw) }
}

use crate::tid::current_tid;

pin_project! {
    /// A wrapper around a future that tracks lifecycle events.
    ///
    /// Created via the `future!` macro, this wrapper tracks:
    /// - Creation
    /// - Each poll call with result (Pending/Ready) and thread ID
    /// - Drop (cancellation if not completed)
    ///
    /// This variant does NOT require `Debug` on the output type.
    /// Use `InstrumentedTaskLog` (via `future!(expr, log = true)`) to log the output value.
    pub struct InstrumentedTask<F: Future> {
        #[pin]
        inner: F,
        task_id: u64,
        call_id: u64,
        completed: bool,
    }

    impl<F: Future> PinnedDrop for InstrumentedTask<F> {
        fn drop(this: Pin<&mut Self>) {
            if !this.completed {
                send_task_event(TaskEvent::Cancelled { task_id: this.task_id, call_id: this.call_id });
            }
        }
    }
}

impl<F: Future> InstrumentedTask<F> {
    /// Create a new instrumented task.
    pub fn new(inner: F, location: &'static str) -> Self {
        let (task_id, is_new) = get_or_create_task_id(location);
        let call_id = TASK_CALL_ID_COUNTER.fetch_add(1, Ordering::Relaxed);

        // Only send TaskCreated if this is a new task (source location)
        if is_new {
            send_task_event(TaskEvent::TaskCreated {
                task_id,
                source: location,
                display_label: None,
            });
        }

        // Always send CallCreated for each invocation
        send_task_event(TaskEvent::CallCreated { task_id, call_id });

        Self {
            inner,
            task_id,
            call_id,
            completed: false,
        }
    }
}

impl<F: Future> Future for InstrumentedTask<F> {
    type Output = F::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();
        let task_id = *this.task_id;
        let call_id = *this.call_id;

        let instrumented_waker = create_instrumented_waker(cx.waker());
        let mut instrumented_cx = Context::from_waker(&instrumented_waker);

        let timestamp = Instant::now();
        let tid = current_tid();
        let result = this.inner.poll(&mut instrumented_cx);

        let poll_result = match &result {
            Poll::Pending => PollResult::Pending,
            Poll::Ready(_) => {
                *this.completed = true;
                PollResult::Ready
            }
        };

        send_task_event(TaskEvent::Polled {
            task_id,
            call_id,
            timestamp,
            tid,
            result: poll_result,
            log_message: None,
        });

        if *this.completed {
            send_task_event(TaskEvent::Completed { task_id, call_id });
        }

        result
    }
}

pin_project! {
    /// A wrapper around a future that tracks lifecycle events including the output value.
    ///
    /// Created via the `future!(expr, log = true)` macro, this wrapper tracks:
    /// - Creation
    /// - Each poll call with result (Pending/Ready with Debug output) and thread ID
    /// - Drop (cancellation if not completed)
    ///
    /// This variant requires `Debug` on the output type to log the value.
    pub struct InstrumentedTaskLog<F: Future> {
        #[pin]
        inner: F,
        task_id: u64,
        call_id: u64,
        completed: bool,
    }

    impl<F: Future> PinnedDrop for InstrumentedTaskLog<F> {
        fn drop(this: Pin<&mut Self>) {
            if !this.completed {
                send_task_event(TaskEvent::Cancelled { task_id: this.task_id, call_id: this.call_id });
            }
        }
    }
}

impl<F: Future> InstrumentedTaskLog<F> {
    /// Create a new instrumented task with logging.
    pub fn new(inner: F, location: &'static str) -> Self {
        let (task_id, is_new) = get_or_create_task_id(location);
        let call_id = TASK_CALL_ID_COUNTER.fetch_add(1, Ordering::Relaxed);

        // Only send TaskCreated if this is a new task (source location)
        if is_new {
            send_task_event(TaskEvent::TaskCreated {
                task_id,
                source: location,
                display_label: None,
            });
        }

        // Always send CallCreated for each invocation
        send_task_event(TaskEvent::CallCreated { task_id, call_id });

        Self {
            inner,
            task_id,
            call_id,
            completed: false,
        }
    }
}

impl<F: Future> Future for InstrumentedTaskLog<F>
where
    F::Output: std::fmt::Debug,
{
    type Output = F::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();
        let task_id = *this.task_id;
        let call_id = *this.call_id;

        let instrumented_waker = create_instrumented_waker(cx.waker());
        let mut instrumented_cx = Context::from_waker(&instrumented_waker);

        let timestamp = Instant::now();
        let tid = current_tid();
        let result = this.inner.poll(&mut instrumented_cx);

        let (poll_result, log_message) = match &result {
            Poll::Pending => (PollResult::Pending, None),
            Poll::Ready(value) => {
                *this.completed = true;
                (PollResult::Ready, Some(format!("{:?}", value)))
            }
        };

        send_task_event(TaskEvent::Polled {
            task_id,
            call_id,
            timestamp,
            tid,
            result: poll_result,
            log_message,
        });

        if *this.completed {
            send_task_event(TaskEvent::Completed { task_id, call_id });
        }

        result
    }
}
