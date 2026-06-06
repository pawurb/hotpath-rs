//! Instrumented wrapper for [`tokio::sync::Mutex`].

use tokio::sync::Mutex as TokioMutex;

use crate::instant::Instant;
use crate::mutexes::{
    elapsed_nanos, register_mutex, send_mutex_event, InstrumentMutex, MutexEvent,
};

/// Instrumented drop-in replacement for [`tokio::sync::Mutex`].
///
/// Not constructed directly - use the [`mutex!`](crate::mutex) macro.
pub struct Mutex<T> {
    inner: TokioMutex<T>,
    id: u32,
}

#[cfg_attr(feature = "hotpath-meta", hotpath_meta::measure_all)]
impl<T> Mutex<T> {
    /// Drop-in constructor for the `hotpath::wrap` prefix migration. Captures the
    /// caller location as the registered source.
    #[track_caller]
    #[deprecated(note = "construct via the hotpath::mutex! macro instead of new()")]
    pub fn new(value: T) -> Self {
        let loc = std::panic::Location::caller();
        let source: &'static str =
            Box::leak(format!("{}:{}", loc.file(), loc.line()).into_boxed_str());
        Self::__new_instrumented(TokioMutex::new(value), source, None)
    }

    #[doc(hidden)]
    pub fn __new_instrumented(
        inner: TokioMutex<T>,
        source: &'static str,
        label: Option<String>,
    ) -> Self {
        let id = register_mutex::<T>(source, label);
        Self { inner, id }
    }

    pub async fn lock(&self) -> MutexGuard<'_, T> {
        // Stamp before acquisition to measure wait time; the guard then measures acquire time.
        let wait_start = Instant::now();
        let inner = self.inner.lock().await;
        self.guard(inner, elapsed_nanos(wait_start))
    }

    pub fn try_lock(&self) -> Result<MutexGuard<'_, T>, tokio::sync::TryLockError> {
        let wait_start = Instant::now();
        self.inner
            .try_lock()
            .map(|inner| self.guard(inner, elapsed_nanos(wait_start)))
    }

    pub fn into_inner(self) -> T {
        self.inner.into_inner()
    }

    pub fn get_mut(&mut self) -> &mut T {
        self.inner.get_mut()
    }

    fn guard<'a>(
        &'a self,
        inner: tokio::sync::MutexGuard<'a, T>,
        wait_nanos: u64,
    ) -> MutexGuard<'a, T> {
        MutexGuard {
            inner: Some(inner),
            start: Instant::now(),
            wait_nanos,
            id: self.id,
        }
    }
}

/// Guard returned by [`Mutex::lock`]. Emits wait and acquire durations on drop.
#[must_use = "if unused the Mutex will immediately unlock"]
pub struct MutexGuard<'a, T> {
    inner: Option<tokio::sync::MutexGuard<'a, T>>,
    start: Instant,
    wait_nanos: u64,
    id: u32,
}

impl<T> std::ops::Deref for MutexGuard<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        self.inner.as_ref().expect("guard held until drop")
    }
}

impl<T> std::ops::DerefMut for MutexGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        self.inner.as_mut().expect("guard held until drop")
    }
}

impl<T> Drop for MutexGuard<'_, T> {
    fn drop(&mut self) {
        // Release the real lock before stamping/sending so the held duration
        // excludes the event-send cost and the lock frees as early as possible.
        drop(self.inner.take());
        let now = Instant::now();
        send_mutex_event(MutexEvent::Released {
            id: self.id,
            wait_nanos: self.wait_nanos,
            acquire_nanos: now.duration_since(self.start).as_nanos() as u64,
            elapsed_ns: crate::lib_on::elapsed_since_start_ns(now),
        });
    }
}

impl<T> InstrumentMutex for TokioMutex<T> {
    type Output = Mutex<T>;
    fn instrument(self, source: &'static str, label: Option<String>) -> Self::Output {
        Mutex::__new_instrumented(self, source, label)
    }
}
