//! Instrumented wrapper for [`parking_lot::RwLock`].

use parking_lot::RwLock as PlRwLock;

use crossbeam_channel::Sender as CbSender;

use crate::instant::Instant;
use crate::rw_locks::{
    register_rw_lock, send_rw_lock_event, InstrumentRwLock, RegisteredRwLock, RwLockEvent,
    RwLockKind,
};

/// Instrumented drop-in replacement for [`parking_lot::RwLock`].
///
/// Not constructed directly - use the [`rw_lock!`](crate::rw_lock) macro.
pub struct RwLock<T> {
    inner: PlRwLock<T>,
    id: u32,
    stats_tx: CbSender<RwLockEvent>,
}

impl<T> RwLock<T> {
    #[doc(hidden)]
    pub fn __new_instrumented(
        inner: PlRwLock<T>,
        source: &'static str,
        label: Option<String>,
    ) -> Self {
        let RegisteredRwLock { id, stats_tx } = register_rw_lock::<T>(source, label);
        Self {
            inner,
            id,
            stats_tx,
        }
    }

    pub fn read(&self) -> RwLockReadGuard<'_, T> {
        // Stamp the clock after acquisition so the guard measures hold time, not wait time.
        self.read_guard(self.inner.read())
    }

    pub fn try_read(&self) -> Option<RwLockReadGuard<'_, T>> {
        self.inner.try_read().map(|inner| self.read_guard(inner))
    }

    pub fn write(&self) -> RwLockWriteGuard<'_, T> {
        self.write_guard(self.inner.write())
    }

    pub fn try_write(&self) -> Option<RwLockWriteGuard<'_, T>> {
        self.inner.try_write().map(|inner| self.write_guard(inner))
    }

    pub fn into_inner(self) -> T {
        self.inner.into_inner()
    }

    pub fn get_mut(&mut self) -> &mut T {
        self.inner.get_mut()
    }

    fn read_guard<'a>(&self, inner: parking_lot::RwLockReadGuard<'a, T>) -> RwLockReadGuard<'a, T> {
        RwLockReadGuard {
            inner,
            start: Instant::now(),
            id: self.id,
            stats_tx: self.stats_tx.clone(),
        }
    }

    fn write_guard<'a>(
        &self,
        inner: parking_lot::RwLockWriteGuard<'a, T>,
    ) -> RwLockWriteGuard<'a, T> {
        RwLockWriteGuard {
            inner,
            start: Instant::now(),
            id: self.id,
            stats_tx: self.stats_tx.clone(),
        }
    }
}

/// Guard returned by [`RwLock::read`]. Emits the hold duration on drop.
#[must_use = "if unused the RwLock will immediately unlock"]
pub struct RwLockReadGuard<'a, T> {
    inner: parking_lot::RwLockReadGuard<'a, T>,
    start: Instant,
    id: u32,
    stats_tx: CbSender<RwLockEvent>,
}

impl<T> std::ops::Deref for RwLockReadGuard<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.inner
    }
}

impl<T> Drop for RwLockReadGuard<'_, T> {
    fn drop(&mut self) {
        let nanos = self.start.elapsed().as_nanos() as u64;
        send_rw_lock_event(
            &self.stats_tx,
            RwLockEvent::Released {
                id: self.id,
                kind: RwLockKind::Read,
                nanos,
            },
        );
    }
}

/// Guard returned by [`RwLock::write`]. Emits the hold duration on drop.
#[must_use = "if unused the RwLock will immediately unlock"]
pub struct RwLockWriteGuard<'a, T> {
    inner: parking_lot::RwLockWriteGuard<'a, T>,
    start: Instant,
    id: u32,
    stats_tx: CbSender<RwLockEvent>,
}

impl<T> std::ops::Deref for RwLockWriteGuard<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.inner
    }
}

impl<T> std::ops::DerefMut for RwLockWriteGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.inner
    }
}

impl<T> Drop for RwLockWriteGuard<'_, T> {
    fn drop(&mut self) {
        let nanos = self.start.elapsed().as_nanos() as u64;
        send_rw_lock_event(
            &self.stats_tx,
            RwLockEvent::Released {
                id: self.id,
                kind: RwLockKind::Write,
                nanos,
            },
        );
    }
}

impl<T> InstrumentRwLock for PlRwLock<T> {
    type Output = RwLock<T>;
    fn instrument(self, source: &'static str, label: Option<String>) -> Self::Output {
        RwLock::__new_instrumented(self, source, label)
    }
}
