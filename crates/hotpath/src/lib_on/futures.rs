//! Futures instrumentation module - prints lifecycle events for debugging.

pub(crate) mod wrapper;

pub use wrapper::{InstrumentedFuture, InstrumentedFutureLog};

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
        const FUTURE_LOC: &'static str = concat!(file!(), ":", line!());
        println!("[FUTURE] Created at {}", FUTURE_LOC);
        $crate::InstrumentFuture::instrument_future($fut, FUTURE_LOC)
    }};

    // With logging: requires Debug
    ($fut:expr, log = true) => {{
        const FUTURE_LOC: &'static str = concat!(file!(), ":", line!());
        println!("[FUTURE] Created at {}", FUTURE_LOC);
        $crate::InstrumentFutureLog::instrument_future_log($fut, FUTURE_LOC)
    }};
}
