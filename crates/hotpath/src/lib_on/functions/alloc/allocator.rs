// Original source: https://github.com/fornwall/allocation-counter
//
// Licensed under either of:
// - Apache License, Version 2.0.
// - MIT/X Consortium License
//
// Modifications:
// - Adjusted to work with hotpath module system
// - Split into feature-specific dispatching allocator

use std::{
    alloc::{GlobalAlloc, Layout, System},
    marker::PhantomData,
};

/// Shared global allocator that dispatches to enabled allocation tracking features
pub struct CountingAllocator<A = System>(PhantomData<A>);

impl<A> CountingAllocator<A> {
    pub const fn new() -> Self {
        Self(PhantomData)
    }
}

impl<A> Default for CountingAllocator<A> {
    fn default() -> Self {
        Self::new()
    }
}

unsafe impl<A> GlobalAlloc for CountingAllocator<A>
where
    A: Default + GlobalAlloc,
{
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        crate::lib_on::functions::alloc::core::track_alloc(layout.size());

        unsafe { A::default().alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        crate::lib_on::functions::alloc::core::track_dealloc(layout.size());

        unsafe {
            A::default().dealloc(ptr, layout);
        }
    }
}
