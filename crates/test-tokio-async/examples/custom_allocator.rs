#[cfg(feature = "hotpath-alloc")]
mod alloc_demo {
    use std::alloc::{GlobalAlloc, Layout, System};
    use std::sync::atomic::{AtomicU64, Ordering};

    static ALLOC_CALLS: AtomicU64 = AtomicU64::new(0);

    #[derive(Default)]
    pub struct TestAllocator;

    unsafe impl GlobalAlloc for TestAllocator {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            ALLOC_CALLS.fetch_add(1, Ordering::Relaxed);
            unsafe { System.alloc(layout) }
        }

        unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
            unsafe { System.dealloc(ptr, layout) }
        }
    }

    #[hotpath::measure]
    fn alloc_work() {
        let buf = vec![0u8; 1024];
        std::hint::black_box(&buf);
    }

    #[hotpath::main(allocator = TestAllocator)]
    pub fn run() -> Result<(), Box<dyn std::error::Error>> {
        let before = ALLOC_CALLS.load(Ordering::Relaxed);
        alloc_work();
        let after = ALLOC_CALLS.load(Ordering::Relaxed);

        assert!(
            after > before,
            "custom allocator should observe an allocation delta"
        );

        Ok(())
    }
}

#[cfg(feature = "hotpath-alloc")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    alloc_demo::run()
}

#[cfg(not(feature = "hotpath-alloc"))]
fn main() {}
