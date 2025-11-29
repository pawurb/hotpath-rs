//! Futures instrumentation module - prints lifecycle events for debugging.

pub(crate) mod wrapper;

pub use wrapper::InstrumentedFuture;

/// Trait for instrumenting futures.
///
/// This trait is not intended for direct use. Use the `future!` macro instead.
#[doc(hidden)]
pub trait InstrumentFuture {
    type Output;
    fn instrument_future(self, source: &'static str) -> Self::Output;
}

impl<F: std::future::Future> InstrumentFuture for F {
    type Output = InstrumentedFuture<F>;

    fn instrument_future(self, source: &'static str) -> Self::Output {
        InstrumentedFuture::new(self, source)
    }
}

/// Instrument a future to inspect future's lifecycle events.
#[macro_export]
macro_rules! future {
    ($fut:expr) => {{
        const FUTURE_LOC: &'static str = concat!(file!(), ":", line!());
        println!("[FUTURE] Created at {}", FUTURE_LOC);
        $crate::InstrumentFuture::instrument_future($fut, FUTURE_LOC)
    }};
}
