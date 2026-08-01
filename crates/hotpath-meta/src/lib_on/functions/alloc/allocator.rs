// Original source: https://github.com/fornwall/allocation-counter
//
// Licensed under either of:
// - Apache License, Version 2.0.
// - MIT/X Consortium License
//
// Modifications:
// - Adjusted to work with hotpath module system
// - Split into feature-specific dispatching allocator

use std::alloc::{GlobalAlloc, Layout, System};

/// Shared global allocator that dispatches to enabled allocation tracking features
pub struct CountingAllocator<A = System>(A);

impl CountingAllocator<System> {
    pub const fn new() -> Self {
        Self(System)
    }
}

impl<A> CountingAllocator<A> {
    pub const fn with(inner: A) -> Self {
        Self(inner)
    }
}

impl<A: Default> Default for CountingAllocator<A> {
    fn default() -> Self {
        Self(A::default())
    }
}

// SAFETY: pure pass-through to the inner allocator `A` - every pointer
// returned by `alloc` comes from `A::alloc` and every `dealloc` forwards the
// caller's `ptr`/`layout` unchanged, so `A`'s GlobalAlloc guarantees carry
// over. The tracking hooks only update thread-local counters and never touch
// the allocation itself.
unsafe impl<A> GlobalAlloc for CountingAllocator<A>
where
    A: GlobalAlloc,
{
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        crate::lib_on::functions::alloc::core::track_alloc(layout.size());

        // SAFETY: caller upholds GlobalAlloc's contract for `layout`; it is
        // forwarded unchanged.
        unsafe { self.0.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        crate::lib_on::functions::alloc::core::track_dealloc(layout.size());

        // SAFETY: caller guarantees `ptr` was allocated by this allocator
        // with `layout`, which means it came from `A::alloc`; both are
        // forwarded unchanged.
        unsafe {
            self.0.dealloc(ptr, layout);
        }
    }
}
