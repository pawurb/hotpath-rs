use std::cell::Cell;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::tid::current_tid;

pub(crate) const MAX_DEPTH: usize = 64;

/// Maximum number of threads we can track (fixed size to avoid allocations in allocator)
const MAX_THREADS: usize = 256;

/// Per-thread allocation statistics (lock-free)
pub(crate) struct ThreadAllocStats {
    /// Thread ID (0 means slot is unused)
    pub(crate) tid: AtomicU64,
    pub(crate) alloc_bytes: AtomicU64,
    pub(crate) dealloc_bytes: AtomicU64,
    pub(crate) alloc_count: AtomicU64,
    pub(crate) dealloc_count: AtomicU64,
}

impl Default for ThreadAllocStats {
    fn default() -> Self {
        Self::new()
    }
}

impl ThreadAllocStats {
    pub(crate) const fn new() -> Self {
        Self {
            tid: AtomicU64::new(0),
            alloc_bytes: AtomicU64::new(0),
            dealloc_bytes: AtomicU64::new(0),
            alloc_count: AtomicU64::new(0),
            dealloc_count: AtomicU64::new(0),
        }
    }
}

#[allow(clippy::declare_interior_mutable_const)]
static THREAD_ALLOC_STATS: [ThreadAllocStats; MAX_THREADS] = {
    const INIT: ThreadAllocStats = ThreadAllocStats::new();
    [INIT; MAX_THREADS]
};

static THREAD_TRACKING_ENABLED: AtomicU64 = AtomicU64::new(0);

/// Initialize the thread allocation tracking system
pub(crate) fn init_thread_alloc_tracking() {
    THREAD_TRACKING_ENABLED.store(1, Ordering::Release);
}

pub(crate) struct ThreadAllocSnapshot {
    pub(crate) alloc_bytes: u64,
    pub(crate) dealloc_bytes: u64,
    pub(crate) alloc_count: u64,
    pub(crate) dealloc_count: u64,
}

/// Get allocation stats for a thread
pub(crate) fn get_thread_alloc_stats(os_tid: u64) -> Option<ThreadAllocSnapshot> {
    if THREAD_TRACKING_ENABLED.load(Ordering::Acquire) == 0 {
        return None;
    }

    for slot in &THREAD_ALLOC_STATS {
        let slot_tid = slot.tid.load(Ordering::Acquire);
        if slot_tid == os_tid {
            return Some(ThreadAllocSnapshot {
                alloc_bytes: slot.alloc_bytes.load(Ordering::Relaxed),
                dealloc_bytes: slot.dealloc_bytes.load(Ordering::Relaxed),
                alloc_count: slot.alloc_count.load(Ordering::Relaxed),
                dealloc_count: slot.dealloc_count.load(Ordering::Relaxed),
            });
        }
        if slot_tid == 0 {
            break;
        }
    }
    None
}

#[inline]
fn get_or_create_slot(tid: u64) -> Option<&'static ThreadAllocStats> {
    for slot in &THREAD_ALLOC_STATS {
        let slot_tid = slot.tid.load(Ordering::Acquire);

        if slot_tid == tid {
            return Some(slot);
        }

        if slot_tid == 0 {
            match slot
                .tid
                .compare_exchange(0, tid, Ordering::AcqRel, Ordering::Acquire)
            {
                Ok(_) => return Some(slot),
                Err(current) if current == tid => return Some(slot),
                Err(_) => continue,
            }
        }
    }
    None
}

pub(crate) struct AllocationInfo {
    pub(crate) bytes_total: Cell<u64>,
    pub(crate) count_total: Cell<u64>,
}

impl std::ops::AddAssign for AllocationInfo {
    fn add_assign(&mut self, other: Self) {
        self.bytes_total
            .set(self.bytes_total.get() + other.bytes_total.get());
        self.count_total
            .set(self.count_total.get() + other.count_total.get());
    }
}

pub(crate) struct AllocationInfoStack {
    pub(crate) depth: Cell<u32>,
    pub(crate) elements: [AllocationInfo; MAX_DEPTH],
    pub(crate) tracking_enabled: Cell<bool>,
}

thread_local! {
    pub(crate) static ALLOCATIONS: AllocationInfoStack = const { AllocationInfoStack {
        depth: Cell::new(0),
        elements: [const { AllocationInfo {
            bytes_total: Cell::new(0),
            count_total: Cell::new(0),
        } }; MAX_DEPTH],
        tracking_enabled: Cell::new(true),
    } };
}

#[inline]
pub(crate) fn track_alloc(size: usize) {
    ALLOCATIONS.with(|stack| {
        if !stack.tracking_enabled.get() {
            return;
        }
        let depth = stack.depth.get() as usize;
        let info = &stack.elements[depth];
        info.bytes_total.set(info.bytes_total.get() + size as u64);
        info.count_total.set(info.count_total.get() + 1);
    });

    if THREAD_TRACKING_ENABLED.load(Ordering::Relaxed) != 0 {
        let tid = current_tid();
        if let Some(slot) = get_or_create_slot(tid) {
            slot.alloc_bytes.fetch_add(size as u64, Ordering::Relaxed);
            slot.alloc_count.fetch_add(1, Ordering::Relaxed);
        }
    }
}

#[inline]
pub(crate) fn track_dealloc(size: usize) {
    if THREAD_TRACKING_ENABLED.load(Ordering::Relaxed) != 0 {
        let tid = current_tid();
        if let Some(slot) = get_or_create_slot(tid) {
            slot.dealloc_bytes.fetch_add(size as u64, Ordering::Relaxed);
            slot.dealloc_count.fetch_add(1, Ordering::Relaxed);
        }
    }
}

#[inline]
pub(crate) fn set_alloc_tracking_enabled(enabled: bool) -> bool {
    ALLOCATIONS.with(|stack| {
        let previous = stack.tracking_enabled.get();
        stack.tracking_enabled.set(enabled);
        previous
    })
}

#[inline]
pub(crate) fn suspend_alloc_tracking() -> bool {
    set_alloc_tracking_enabled(false)
}

#[inline]
pub(crate) fn resume_alloc_tracking(previous_enabled: bool) {
    let _ = set_alloc_tracking_enabled(previous_enabled);
}

#[cfg(test)]
#[inline]
pub(crate) fn alloc_tracking_enabled() -> bool {
    ALLOCATIONS.with(|stack| stack.tracking_enabled.get())
}
