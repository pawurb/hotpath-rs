//! Instrumented Future wrapper that tracks lifecycle events.

use crate::output::format_debug_truncated;

use crate::functions::AsyncAllocBridge;
use crate::lib_on::futures::{
    get_futures_event_tx, get_or_create_future_id, FutureEvent, PollResult, FUTURE_CALL_ID_COUNTER,
};
use crossbeam_channel::Sender as CbSender;
use pin_project_lite::pin_project;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::task::{Context, Poll};

use crate::instant::Instant;

#[cfg(feature = "hotpath-alloc-meta")]
#[inline]
fn measure_poll_alloc<R>(poll_fn: impl FnOnce() -> R) -> (R, Option<u64>, Option<u64>) {
    crate::functions::alloc::guard::push_alloc_stack();

    let result = poll_fn();

    let (bytes, count) = crate::functions::alloc::guard::pop_alloc_stack();

    (result, Some(bytes), Some(count))
}

#[cfg(not(feature = "hotpath-alloc-meta"))]
#[inline]
fn measure_poll_alloc<R>(poll_fn: impl FnOnce() -> R) -> (R, Option<u64>, Option<u64>) {
    (poll_fn(), None, None)
}

pin_project! {
    /// A wrapper around a future that tracks lifecycle events.
    ///
    /// Created via the `future!` macro, this wrapper tracks:
    /// - Creation
    /// - Each poll call with result (Pending/Ready) and duration
    /// - Memory allocations per poll (when `hotpath-alloc` feature is enabled)
    /// - Drop (cancellation if not completed)
    ///
    /// This variant does NOT require `Debug` on the output type.
    /// Use `InstrumentedFutureLog` (via `future!(expr, log = true)`) to log the output value.
    pub struct InstrumentedFuture<F: Future> {
        #[pin]
        inner: F,
        stats_tx: Option<&'static CbSender<FutureEvent>>,
        future_id: u32,
        call_id: u32,
        completed: bool,
        visible: bool,
        alloc_bridge: Option<Arc<AsyncAllocBridge>>,
    }

    impl<F: Future> PinnedDrop for InstrumentedFuture<F> {
        fn drop(this: Pin<&mut Self>) {
            if let (true, false, Some(stats_tx)) = (this.visible, this.completed, this.stats_tx) {
                let _ = stats_tx.send(FutureEvent::Cancelled {
                    future_id: this.future_id,
                    call_id: this.call_id,
                });
            }
        }
    }
}

impl<F: Future> InstrumentedFuture<F> {
    pub(crate) fn new(
        inner: F,
        location: &'static str,
        label: Option<String>,
        alloc_bridge: Option<Arc<AsyncAllocBridge>>,
        visible: bool,
    ) -> Self {
        let _suspend = crate::lib_on::SuspendAllocTracking::new();

        let (stats_tx, future_id, call_id) = if visible {
            let stats_tx = get_futures_event_tx();
            let (future_id, is_new) = get_or_create_future_id(location);
            let call_id = FUTURE_CALL_ID_COUNTER.fetch_add(1, Ordering::Relaxed);

            if is_new {
                let _ = stats_tx.send(FutureEvent::Created {
                    future_id,
                    source: location,
                    display_label: label,
                });
            }

            let _ = stats_tx.send(FutureEvent::CallCreated { future_id, call_id });
            (Some(stats_tx), future_id, call_id)
        } else {
            (None, 0, 0)
        };

        drop(_suspend);

        Self {
            inner,
            stats_tx,
            future_id,
            call_id,
            completed: false,
            visible,
            alloc_bridge,
        }
    }
}

impl<F: Future> Future for InstrumentedFuture<F> {
    type Output = F::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();
        let visible = *this.visible;

        // Don't instrument future unless visible, only collect alloc data
        if !visible {
            let (result, poll_alloc_bytes, poll_alloc_count) =
                measure_poll_alloc(|| this.inner.poll(cx));
            if let (Some(bytes), Some(count), Some(bridge)) = (
                poll_alloc_bytes,
                poll_alloc_count,
                this.alloc_bridge.as_ref(),
            ) {
                bridge.add(bytes, count);
            }
            if result.is_ready() {
                *this.completed = true;
            }
            return result;
        }

        let future_id = *this.future_id;
        let call_id = *this.call_id;
        let stats_tx = this
            .stats_tx
            .expect("visible instrumented futures always cache a sender");

        let start = Instant::now();
        let (result, poll_alloc_bytes, poll_alloc_count) =
            measure_poll_alloc(|| this.inner.poll(cx));
        let poll_duration_ns = start.elapsed().as_nanos() as u64;
        if let (Some(bytes), Some(count), Some(bridge)) = (
            poll_alloc_bytes,
            poll_alloc_count,
            this.alloc_bridge.as_ref(),
        ) {
            bridge.add(bytes, count);
        }

        let poll_result = match &result {
            Poll::Pending => PollResult::Pending,
            Poll::Ready(_) => {
                *this.completed = true;
                PollResult::Ready
            }
        };

        {
            let _suspend = crate::lib_on::SuspendAllocTracking::new();
            let _ = stats_tx.send(FutureEvent::Polled {
                future_id,
                call_id,
                result: poll_result,
                poll_duration_ns,
                poll_alloc_bytes,
                poll_alloc_count,
            });

            if *this.completed {
                let _ = stats_tx.send(FutureEvent::Completed {
                    future_id,
                    call_id,
                    log_message: None,
                });
            }
        }

        result
    }
}

pin_project! {
    /// A wrapper around a future that tracks lifecycle events including the output value.
    ///
    /// Created via the `future!(expr, log = true)` macro, this wrapper tracks:
    /// - Creation
    /// - Each poll call with result (Pending/Ready with Debug output) and duration
    /// - Memory allocations per poll (when `hotpath-alloc` feature is enabled)
    /// - Drop (cancellation if not completed)
    ///
    /// This variant requires `Debug` on the output type to log the value.
    pub struct InstrumentedFutureLog<F: Future> {
        #[pin]
        inner: F,
        stats_tx: Option<&'static CbSender<FutureEvent>>,
        future_id: u32,
        call_id: u32,
        completed: bool,
        visible: bool,
        alloc_bridge: Option<Arc<AsyncAllocBridge>>,
    }

    impl<F: Future> PinnedDrop for InstrumentedFutureLog<F> {
        fn drop(this: Pin<&mut Self>) {
            if let (true, false, Some(stats_tx)) = (this.visible, this.completed, this.stats_tx) {
                let _ = stats_tx.send(FutureEvent::Cancelled {
                    future_id: this.future_id,
                    call_id: this.call_id,
                });
            }
        }
    }
}

impl<F: Future> InstrumentedFutureLog<F> {
    /// Create a new instrumented future with logging.
    pub(crate) fn new(
        inner: F,
        location: &'static str,
        label: Option<String>,
        alloc_bridge: Option<Arc<AsyncAllocBridge>>,
        visible: bool,
    ) -> Self {
        let _suspend = crate::lib_on::SuspendAllocTracking::new();

        let (stats_tx, future_id, call_id) = if visible {
            let stats_tx = get_futures_event_tx();
            let (future_id, is_new) = get_or_create_future_id(location);
            let call_id = FUTURE_CALL_ID_COUNTER.fetch_add(1, Ordering::Relaxed);

            if is_new {
                let _ = stats_tx.send(FutureEvent::Created {
                    future_id,
                    source: location,
                    display_label: label,
                });
            }

            let _ = stats_tx.send(FutureEvent::CallCreated { future_id, call_id });
            (Some(stats_tx), future_id, call_id)
        } else {
            (None, 0, 0)
        };

        drop(_suspend);

        Self {
            inner,
            stats_tx,
            future_id,
            call_id,
            completed: false,
            visible,
            alloc_bridge,
        }
    }
}

impl<F: Future> Future for InstrumentedFutureLog<F>
where
    F::Output: std::fmt::Debug,
{
    type Output = F::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();
        let visible = *this.visible;

        if !visible {
            let (result, poll_alloc_bytes, poll_alloc_count) =
                measure_poll_alloc(|| this.inner.poll(cx));
            if let (Some(bytes), Some(count), Some(bridge)) = (
                poll_alloc_bytes,
                poll_alloc_count,
                this.alloc_bridge.as_ref(),
            ) {
                bridge.add(bytes, count);
            }
            if result.is_ready() {
                *this.completed = true;
            }
            return result;
        }

        let future_id = *this.future_id;
        let call_id = *this.call_id;
        let stats_tx = this
            .stats_tx
            .expect("visible instrumented futures always cache a sender");

        let start = Instant::now();
        let (result, poll_alloc_bytes, poll_alloc_count) =
            measure_poll_alloc(|| this.inner.poll(cx));
        let poll_duration_ns = start.elapsed().as_nanos() as u64;
        if let (Some(bytes), Some(count), Some(bridge)) = (
            poll_alloc_bytes,
            poll_alloc_count,
            this.alloc_bridge.as_ref(),
        ) {
            bridge.add(bytes, count);
        }

        let (poll_result, log_message) = match &result {
            Poll::Pending => (PollResult::Pending, None),
            Poll::Ready(value) => {
                *this.completed = true;
                (PollResult::Ready, Some(format_debug_truncated(value)))
            }
        };

        {
            let _suspend = crate::lib_on::SuspendAllocTracking::new();
            let _ = stats_tx.send(FutureEvent::Polled {
                future_id,
                call_id,
                result: poll_result,
                poll_duration_ns,
                poll_alloc_bytes,
                poll_alloc_count,
            });

            if *this.completed {
                let _ = stats_tx.send(FutureEvent::Completed {
                    future_id,
                    call_id,
                    log_message,
                });
            }
        }

        result
    }
}
