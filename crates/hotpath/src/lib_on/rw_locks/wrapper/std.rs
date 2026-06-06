//! Instrumented wrapper for [`std::sync::RwLock`].

use std::sync::RwLock as StdRwLock;

use crate::instant::Instant;
use crate::rw_locks::{
    elapsed_nanos, register_rw_lock, send_rw_lock_event, InstrumentRwLock, RwLockEvent, RwLockKind,
};

/// Instrumented drop-in replacement for [`std::sync::RwLock`].
///
/// Not constructed directly - use the [`rw_lock!`](crate::rw_lock) macro.
pub struct RwLock<T> {
    inner: StdRwLock<T>,
    id: u32,
}

#[cfg_attr(feature = "hotpath-meta", hotpath_meta::measure_all)]
impl<T> RwLock<T> {
    /// Drop-in constructor for the `hotpath::wrap` prefix migration. Captures the
    /// caller location as the registered source.
    #[track_caller]
    #[deprecated(note = "construct via the hotpath::rw_lock! macro instead of new()")]
    pub fn new(value: T) -> Self {
        let loc = std::panic::Location::caller();
        let source: &'static str =
            Box::leak(format!("{}:{}", loc.file(), loc.line()).into_boxed_str());
        Self::__new_instrumented(StdRwLock::new(value), source, None)
    }

    #[doc(hidden)]
    pub fn __new_instrumented(
        inner: StdRwLock<T>,
        source: &'static str,
        label: Option<String>,
    ) -> Self {
        let id = register_rw_lock::<T>(source, label);
        Self { inner, id }
    }

    pub fn read(&self) -> std::sync::LockResult<RwLockReadGuard<'_, T>> {
        // Stamp before acquisition to measure wait time; the guard then measures acquire time.
        let wait_start = Instant::now();
        match self.inner.read() {
            Ok(inner) => Ok(self.read_guard(inner, elapsed_nanos(wait_start))),
            Err(poison) => Err(std::sync::PoisonError::new(
                self.read_guard(poison.into_inner(), elapsed_nanos(wait_start)),
            )),
        }
    }

    pub fn try_read(&self) -> std::sync::TryLockResult<RwLockReadGuard<'_, T>> {
        let wait_start = Instant::now();
        match self.inner.try_read() {
            Ok(inner) => Ok(self.read_guard(inner, elapsed_nanos(wait_start))),
            Err(std::sync::TryLockError::Poisoned(poison)) => Err(
                std::sync::TryLockError::Poisoned(std::sync::PoisonError::new(
                    self.read_guard(poison.into_inner(), elapsed_nanos(wait_start)),
                )),
            ),
            Err(std::sync::TryLockError::WouldBlock) => Err(std::sync::TryLockError::WouldBlock),
        }
    }

    pub fn write(&self) -> std::sync::LockResult<RwLockWriteGuard<'_, T>> {
        let wait_start = Instant::now();
        match self.inner.write() {
            Ok(inner) => Ok(self.write_guard(inner, elapsed_nanos(wait_start))),
            Err(poison) => Err(std::sync::PoisonError::new(
                self.write_guard(poison.into_inner(), elapsed_nanos(wait_start)),
            )),
        }
    }

    pub fn try_write(&self) -> std::sync::TryLockResult<RwLockWriteGuard<'_, T>> {
        let wait_start = Instant::now();
        match self.inner.try_write() {
            Ok(inner) => Ok(self.write_guard(inner, elapsed_nanos(wait_start))),
            Err(std::sync::TryLockError::Poisoned(poison)) => Err(
                std::sync::TryLockError::Poisoned(std::sync::PoisonError::new(
                    self.write_guard(poison.into_inner(), elapsed_nanos(wait_start)),
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

    fn read_guard<'a>(
        &'a self,
        inner: std::sync::RwLockReadGuard<'a, T>,
        wait_nanos: u64,
    ) -> RwLockReadGuard<'a, T> {
        RwLockReadGuard {
            inner: Some(inner),
            start: Instant::now(),
            wait_nanos,
            id: self.id,
        }
    }

    fn write_guard<'a>(
        &'a self,
        inner: std::sync::RwLockWriteGuard<'a, T>,
        wait_nanos: u64,
    ) -> RwLockWriteGuard<'a, T> {
        RwLockWriteGuard {
            inner: Some(inner),
            start: Instant::now(),
            wait_nanos,
            id: self.id,
        }
    }
}

/// Guard returned by [`RwLock::read`]. Emits wait and acquire durations on drop.
#[must_use = "if unused the RwLock will immediately unlock"]
pub struct RwLockReadGuard<'a, T> {
    inner: Option<std::sync::RwLockReadGuard<'a, T>>,
    start: Instant,
    wait_nanos: u64,
    id: u32,
}

impl<T> std::ops::Deref for RwLockReadGuard<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        self.inner.as_ref().expect("guard held until drop")
    }
}

impl<T> Drop for RwLockReadGuard<'_, T> {
    fn drop(&mut self) {
        // Release the real lock before stamping/sending so the held duration
        // excludes the event-send cost and the lock frees as early as possible.
        drop(self.inner.take());
        let now = Instant::now();
        send_rw_lock_event(RwLockEvent::Released {
            id: self.id,
            kind: RwLockKind::Read,
            wait_nanos: self.wait_nanos,
            acquire_nanos: now.duration_since(self.start).as_nanos() as u64,
            elapsed_ns: crate::lib_on::elapsed_since_start_ns(now),
        });
    }
}

/// Guard returned by [`RwLock::write`]. Emits wait and acquire durations on drop.
#[must_use = "if unused the RwLock will immediately unlock"]
pub struct RwLockWriteGuard<'a, T> {
    inner: Option<std::sync::RwLockWriteGuard<'a, T>>,
    start: Instant,
    wait_nanos: u64,
    id: u32,
}

impl<T> std::ops::Deref for RwLockWriteGuard<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        self.inner.as_ref().expect("guard held until drop")
    }
}

impl<T> std::ops::DerefMut for RwLockWriteGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        self.inner.as_mut().expect("guard held until drop")
    }
}

impl<T> Drop for RwLockWriteGuard<'_, T> {
    fn drop(&mut self) {
        // Release the real lock before stamping/sending so the held duration
        // excludes the event-send cost and the lock frees as early as possible.
        drop(self.inner.take());
        let now = Instant::now();
        send_rw_lock_event(RwLockEvent::Released {
            id: self.id,
            kind: RwLockKind::Write,
            wait_nanos: self.wait_nanos,
            acquire_nanos: now.duration_since(self.start).as_nanos() as u64,
            elapsed_ns: crate::lib_on::elapsed_since_start_ns(now),
        });
    }
}

impl<T> InstrumentRwLock for StdRwLock<T> {
    type Output = RwLock<T>;
    fn instrument(self, source: &'static str, label: Option<String>) -> Self::Output {
        RwLock::__new_instrumented(self, source, label)
    }
}
