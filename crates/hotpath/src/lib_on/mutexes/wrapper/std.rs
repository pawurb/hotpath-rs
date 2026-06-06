//! Instrumented wrapper for [`std::sync::Mutex`].

use std::sync::Mutex as StdMutex;

use crate::instant::Instant;
use crate::mutexes::{
    elapsed_nanos, register_mutex, send_mutex_event, InstrumentMutex, MutexEvent,
};

/// Instrumented drop-in replacement for [`std::sync::Mutex`].
///
/// Not constructed directly - use the [`mutex!`](crate::mutex) macro.
pub struct Mutex<T> {
    inner: StdMutex<T>,
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
        Self::__new_instrumented(StdMutex::new(value), source, None)
    }

    #[doc(hidden)]
    pub fn __new_instrumented(
        inner: StdMutex<T>,
        source: &'static str,
        label: Option<String>,
    ) -> Self {
        let id = register_mutex::<T>(source, label);
        Self { inner, id }
    }

    pub fn lock(&self) -> std::sync::LockResult<MutexGuard<'_, T>> {
        // Stamp before acquisition to measure wait time; the guard then measures acquire time.
        let wait_start = Instant::now();
        match self.inner.lock() {
            Ok(inner) => Ok(self.guard(inner, elapsed_nanos(wait_start))),
            Err(poison) => Err(std::sync::PoisonError::new(
                self.guard(poison.into_inner(), elapsed_nanos(wait_start)),
            )),
        }
    }

    pub fn try_lock(&self) -> std::sync::TryLockResult<MutexGuard<'_, T>> {
        let wait_start = Instant::now();
        match self.inner.try_lock() {
            Ok(inner) => Ok(self.guard(inner, elapsed_nanos(wait_start))),
            Err(std::sync::TryLockError::Poisoned(poison)) => Err(
                std::sync::TryLockError::Poisoned(std::sync::PoisonError::new(
                    self.guard(poison.into_inner(), elapsed_nanos(wait_start)),
                )),
            ),
            Err(std::sync::TryLockError::WouldBlock) => Err(std::sync::TryLockError::WouldBlock),
        }
    }

    pub fn into_inner(self) -> std::sync::LockResult<T> {
        self.inner.into_inner()
    }

    pub fn get_mut(&mut self) -> std::sync::LockResult<&mut T> {
        self.inner.get_mut()
    }

    fn guard<'a>(
        &'a self,
        inner: std::sync::MutexGuard<'a, T>,
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
    inner: Option<std::sync::MutexGuard<'a, T>>,
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

impl<T> InstrumentMutex for StdMutex<T> {
    type Output = Mutex<T>;
    fn instrument(self, source: &'static str, label: Option<String>) -> Self::Output {
        Mutex::__new_instrumented(self, source, label)
    }
}
