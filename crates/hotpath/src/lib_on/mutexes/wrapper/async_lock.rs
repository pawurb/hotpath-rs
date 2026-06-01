//! Instrumented wrapper for [`async_lock::Mutex`].

use async_lock::Mutex as AlMutex;

use crossbeam_channel::Sender as CbSender;

use crate::instant::Instant;
use crate::mutexes::{
    elapsed_nanos, register_mutex, send_mutex_event, InstrumentMutex, MutexEvent, RegisteredMutex,
};

/// Instrumented drop-in replacement for [`async_lock::Mutex`].
///
/// Not constructed directly - use the [`mutex!`](crate::mutex) macro.
pub struct Mutex<T> {
    inner: AlMutex<T>,
    id: u32,
    stats_tx: CbSender<MutexEvent>,
}

impl<T> Mutex<T> {
    /// Drop-in constructor for the `hotpath::wrap` prefix migration. Captures the
    /// caller location as the registered source.
    #[track_caller]
    #[deprecated(note = "construct via the hotpath::mutex! macro instead of new()")]
    pub fn new(value: T) -> Self {
        let loc = std::panic::Location::caller();
        let source: &'static str =
            Box::leak(format!("{}:{}", loc.file(), loc.line()).into_boxed_str());
        Self::__new_instrumented(AlMutex::new(value), source, None)
    }

    #[doc(hidden)]
    pub fn __new_instrumented(
        inner: AlMutex<T>,
        source: &'static str,
        label: Option<String>,
    ) -> Self {
        let RegisteredMutex { id, stats_tx } = register_mutex::<T>(source, label);
        Self {
            inner,
            id,
            stats_tx,
        }
    }

    pub async fn lock(&self) -> MutexGuard<'_, T> {
        // Stamp before acquisition to measure wait time; the guard then measures acquire time.
        let wait_start = Instant::now();
        let inner = self.inner.lock().await;
        self.guard(inner, elapsed_nanos(wait_start))
    }

    pub fn try_lock(&self) -> Option<MutexGuard<'_, T>> {
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
        &self,
        inner: async_lock::MutexGuard<'a, T>,
        wait_nanos: u64,
    ) -> MutexGuard<'a, T> {
        MutexGuard {
            inner,
            start: Instant::now(),
            wait_nanos,
            id: self.id,
            stats_tx: self.stats_tx.clone(),
        }
    }
}

/// Guard returned by [`Mutex::lock`]. Emits wait and acquire durations on drop.
#[must_use = "if unused the Mutex will immediately unlock"]
pub struct MutexGuard<'a, T> {
    inner: async_lock::MutexGuard<'a, T>,
    start: Instant,
    wait_nanos: u64,
    id: u32,
    stats_tx: CbSender<MutexEvent>,
}

impl<T> std::ops::Deref for MutexGuard<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.inner
    }
}

impl<T> std::ops::DerefMut for MutexGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.inner
    }
}

impl<T> Drop for MutexGuard<'_, T> {
    fn drop(&mut self) {
        send_mutex_event(
            &self.stats_tx,
            MutexEvent::Released {
                id: self.id,
                wait_nanos: self.wait_nanos,
                acquire_nanos: elapsed_nanos(self.start),
            },
        );
    }
}

impl<T> InstrumentMutex for AlMutex<T> {
    type Output = Mutex<T>;
    fn instrument(self, source: &'static str, label: Option<String>) -> Self::Output {
        Mutex::__new_instrumented(self, source, label)
    }
}
