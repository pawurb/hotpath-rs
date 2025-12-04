use std::cell::Cell;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::tid::current_tid;

pub const MAX_DEPTH: usize = 64;

/// Maximum number of threads we can track (fixed size to avoid allocations in allocator)
const MAX_THREADS: usize = 256;

/// Per-thread allocation statistics (lock-free)
pub struct ThreadAllocStats {
    /// Thread ID (0 means slot is unused)
    pub tid: AtomicU64,
    pub alloc_bytes: AtomicU64,
    pub dealloc_bytes: AtomicU64,
}

impl Default for ThreadAllocStats {
    fn default() -> Self {
        Self::new()
    }
}

impl ThreadAllocStats {
    pub const fn new() -> Self {
        Self {
            tid: AtomicU64::new(0),
            alloc_bytes: AtomicU64::new(0),
            dealloc_bytes: AtomicU64::new(0),
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
pub fn init_thread_alloc_tracking() {
    THREAD_TRACKING_ENABLED.store(1, Ordering::Release);
}

/// Get allocation stats for a thread
pub fn get_thread_alloc_stats(os_tid: u64) -> Option<(u64, u64)> {
    if THREAD_TRACKING_ENABLED.load(Ordering::Acquire) == 0 {
        return None;
    }

    for slot in &THREAD_ALLOC_STATS {
        let slot_tid = slot.tid.load(Ordering::Acquire);
        if slot_tid == os_tid {
            return Some((
                slot.alloc_bytes.load(Ordering::Relaxed),
                slot.dealloc_bytes.load(Ordering::Relaxed),
            ));
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

pub struct AllocationInfo {
    pub bytes_total: Cell<u64>,
    pub count_total: Cell<u64>,
    pub unsupported_async: Cell<bool>,
}

impl std::ops::AddAssign for AllocationInfo {
    fn add_assign(&mut self, other: Self) {
        self.bytes_total
            .set(self.bytes_total.get() + other.bytes_total.get());
        self.count_total
            .set(self.count_total.get() + other.count_total.get());
        self.unsupported_async
            .set(self.unsupported_async.get() | other.unsupported_async.get());
    }
}

pub struct AllocationInfoStack {
    pub depth: Cell<u32>,
    pub elements: [AllocationInfo; MAX_DEPTH],
    pub tracking_enabled: Cell<bool>,
}

thread_local! {
    pub static ALLOCATIONS: AllocationInfoStack = const { AllocationInfoStack {
        depth: Cell::new(0),
        elements: [const { AllocationInfo {
            bytes_total: Cell::new(0),
            count_total: Cell::new(0),
            unsupported_async: Cell::new(false)
        } }; MAX_DEPTH],
        tracking_enabled: Cell::new(true),
    } };
}

#[inline]
pub fn track_alloc(size: usize) {
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
        }
    }
}

#[inline]
pub fn track_dealloc(size: usize) {
    if THREAD_TRACKING_ENABLED.load(Ordering::Relaxed) != 0 {
        let tid = current_tid();
        if let Some(slot) = get_or_create_slot(tid) {
            slot.dealloc_bytes.fetch_add(size as u64, Ordering::Relaxed);
        }
    }
}
