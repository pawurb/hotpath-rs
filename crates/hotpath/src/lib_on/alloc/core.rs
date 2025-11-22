use std::cell::Cell;

pub const MAX_DEPTH: usize = 64;

/// Allocation info tracking both total bytes and count
pub struct AllocationInfo {
    /// The total amount of bytes allocated during a [measure()] call.
    pub bytes_total: Cell<u64>,

    /// The total number of allocations during a [measure()] call.
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

/// Called by the shared global allocator to track allocations
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
}
