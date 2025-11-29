//! Instrumented Future wrapper that prints lifecycle events.

use super::{FutureEvent, FUTURE_ID_COUNTER};
use pin_project_lite::pin_project;
use std::future::Future;
use std::mem::ManuallyDrop;
use std::pin::Pin;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

struct WakerData {
    inner: Waker,
    location: &'static str,
}

fn waker_clone(data: *const ()) -> RawWaker {
    let arc = ManuallyDrop::new(unsafe { Arc::from_raw(data as *const WakerData) });
    let cloned = Arc::clone(&arc);
    RawWaker::new(Arc::into_raw(cloned) as *const (), &VTABLE)
}

fn waker_wake(data: *const ()) {
    // Increment first for panic safety, then from_raw "takes" ownership
    unsafe { Arc::increment_strong_count(data as *const WakerData) };
    let arc = unsafe { Arc::from_raw(data as *const WakerData) };
    println!("[FUTURE {}] Wake", arc.location);
    arc.inner.wake_by_ref();
    // arc drops here, decrementing count back - net effect: original consumed
}

fn waker_wake_by_ref(data: *const ()) {
    // Use ManuallyDrop for panic safety - even if wake_by_ref panics, we won't double-free
    let arc = ManuallyDrop::new(unsafe { Arc::from_raw(data as *const WakerData) });
    println!("[FUTURE {}] Wake", arc.location);
    arc.inner.wake_by_ref();
}

fn waker_drop(data: *const ()) {
    unsafe {
        Arc::from_raw(data as *const WakerData);
        // Arc drops here, decrementing refcount
    }
}

static VTABLE: RawWakerVTable =
    RawWakerVTable::new(waker_clone, waker_wake, waker_wake_by_ref, waker_drop);

fn create_instrumented_waker(waker: &Waker, location: &'static str) -> Waker {
    let data = Arc::new(WakerData {
        inner: waker.clone(),
        location,
    });
    let raw = RawWaker::new(Arc::into_raw(data) as *const (), &VTABLE);
    unsafe { Waker::from_raw(raw) }
}

pin_project! {
    /// A wrapper around a future that prints lifecycle events.
    ///
    /// Created via the `future!` macro, this wrapper tracks:
    /// - Creation (printed by the macro)
    /// - Each poll call with result (Pending/Ready)
    /// - Wake events (via instrumented waker)
    /// - Drop (via PinnedDrop)
    ///
    /// This variant does NOT require `Debug` on the output type.
    /// Use `InstrumentedFutureLog` (via `future!(expr, log = true)`) to print the output value.
    pub struct InstrumentedFuture<F> {
        #[pin]
        inner: F,
        id: u64,
        location: &'static str,
        poll_count: usize,
    }

    impl<F> PinnedDrop for InstrumentedFuture<F> {
        fn drop(this: Pin<&mut Self>) {
            println!("[FUTURE {} id={}] Dropped", this.location, this.id);
        }
    }
}

impl<F> InstrumentedFuture<F> {
    /// Create a new instrumented future.
    ///
    /// Note: The "Created" message is printed by the `future!` macro,
    /// not here, to capture the correct source location.
    pub fn new(inner: F, location: &'static str) -> Self {
        let id = FUTURE_ID_COUNTER.fetch_add(1, Ordering::Relaxed);

        // Create the FutureEvent::Created event
        let _event = FutureEvent::Created {
            id,
            source: location,
        };

        Self {
            inner,
            id,
            location,
            poll_count: 0,
        }
    }
}

impl<F: Future> Future for InstrumentedFuture<F> {
    type Output = F::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();
        *this.poll_count += 1;
        let count = *this.poll_count;
        let id = *this.id;
        let location = *this.location;

        // Create an instrumented waker that will log wake events
        let instrumented_waker = create_instrumented_waker(cx.waker(), location);
        let mut instrumented_cx = Context::from_waker(&instrumented_waker);

        let result = this.inner.poll(&mut instrumented_cx);

        match &result {
            Poll::Pending => println!("[FUTURE {} id={}] Poll #{} -> Pending", location, id, count),
            Poll::Ready(_) => println!("[FUTURE {} id={}] Poll #{} -> Ready", location, id, count),
        }

        result
    }
}

pin_project! {
    /// A wrapper around a future that prints lifecycle events including the output value.
    ///
    /// Created via the `future!(expr, log = true)` macro, this wrapper tracks:
    /// - Creation (printed by the macro)
    /// - Each poll call with result (Pending/Ready with Debug output)
    /// - Wake events (via instrumented waker)
    /// - Drop (via PinnedDrop)
    ///
    /// This variant requires `Debug` on the output type to print the value.
    pub struct InstrumentedFutureLog<F> {
        #[pin]
        inner: F,
        id: u64,
        location: &'static str,
        poll_count: usize,
    }

    impl<F> PinnedDrop for InstrumentedFutureLog<F> {
        fn drop(this: Pin<&mut Self>) {
            println!("[FUTURE {} id={}] Dropped", this.location, this.id);
        }
    }
}

impl<F> InstrumentedFutureLog<F> {
    /// Create a new instrumented future with logging.
    pub fn new(inner: F, location: &'static str) -> Self {
        let id = FUTURE_ID_COUNTER.fetch_add(1, Ordering::Relaxed);

        // Create the FutureEvent::Created event
        let _event = FutureEvent::Created {
            id,
            source: location,
        };

        Self {
            inner,
            id,
            location,
            poll_count: 0,
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
        *this.poll_count += 1;
        let count = *this.poll_count;
        let id = *this.id;
        let location = *this.location;

        // Create an instrumented waker that will log wake events
        let instrumented_waker = create_instrumented_waker(cx.waker(), location);
        let mut instrumented_cx = Context::from_waker(&instrumented_waker);

        let result = this.inner.poll(&mut instrumented_cx);

        match &result {
            Poll::Pending => println!("[FUTURE {} id={}] Poll #{} -> Pending", location, id, count),
            Poll::Ready(value) => {
                println!(
                    "[FUTURE {} id={}] Poll #{} -> Ready({:?})",
                    location, id, count, value
                )
            }
        }

        result
    }
}
