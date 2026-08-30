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

/// Shared global allocator that tracks allocations when the `hotpath-alloc-meta`
/// feature is enabled and forwards to the inner allocator unchanged otherwise.
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
// returned by `alloc`/`alloc_zeroed`/`realloc` comes from the corresponding
// method of `A`, and `dealloc`/`realloc` forward the caller's
// `ptr`/`layout`/`new_size` unchanged, so `A`'s GlobalAlloc guarantees carry
// over. The tracking hooks only update thread-local counters and never touch
// the allocation itself.
unsafe impl<A> GlobalAlloc for CountingAllocator<A>
where
    A: GlobalAlloc,
{
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        #[cfg(feature = "hotpath-alloc-meta")]
        crate::lib_on::functions::alloc::core::track_alloc(layout.size());

        // SAFETY: caller upholds GlobalAlloc's contract for `layout`; it is
        // forwarded unchanged.
        unsafe { self.0.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        #[cfg(feature = "hotpath-alloc-meta")]
        crate::lib_on::functions::alloc::core::track_dealloc(layout.size());

        // SAFETY: caller guarantees `ptr` was allocated by this allocator
        // with `layout`, which means it came from `A::alloc`; both are
        // forwarded unchanged.
        unsafe {
            self.0.dealloc(ptr, layout);
        }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        #[cfg(feature = "hotpath-alloc-meta")]
        crate::lib_on::functions::alloc::core::track_alloc(layout.size());

        // SAFETY: caller upholds GlobalAlloc's contract for `layout`; it is
        // forwarded unchanged.
        unsafe { self.0.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: caller guarantees `ptr` was allocated by this allocator
        // with `layout` and that `new_size` is valid per GlobalAlloc's
        // contract; all three are forwarded unchanged.
        let new_ptr = unsafe { self.0.realloc(ptr, layout, new_size) };

        // Track only on success: a failed realloc leaves the old block alive,
        // so recording a dealloc for it would over-count frees.
        #[cfg(feature = "hotpath-alloc-meta")]
        if !new_ptr.is_null() {
            crate::lib_on::functions::alloc::core::track_dealloc(layout.size());
            crate::lib_on::functions::alloc::core::track_alloc(new_size);
        }
        new_ptr
    }
}
