pub struct MeasurementGuard {
    name: &'static str,
    wrapper: bool,
    unsupported_async: bool,
    tid: u64,
}

impl MeasurementGuard {
    #[inline]
    pub fn new(name: &'static str, wrapper: bool, unsupported_async: bool) -> Self {
        if !unsupported_async {
            super::core::ALLOCATIONS.with(|stack| {
                let current_depth = stack.depth.get();
                stack.depth.set(current_depth + 1);
                assert!((stack.depth.get() as usize) < super::core::MAX_DEPTH);
                let depth = stack.depth.get() as usize;
                stack.elements[depth].bytes_total.set(0);
                stack.elements[depth].count_total.set(0);
                stack.elements[depth].unsupported_async.set(false);
            });
        }

        Self {
            name,
            wrapper,
            unsupported_async,
            tid: crate::tid::current_tid(),
        }
    }
}

impl Drop for MeasurementGuard {
    #[inline]
    fn drop(&mut self) {
        let cross_thread = crate::tid::current_tid() != self.tid;

        let (bytes_total, count_total, unsupported_async) =
            if self.unsupported_async || cross_thread {
                (0, 0, self.unsupported_async)
            } else {
                super::core::ALLOCATIONS.with(|stack| {
                    let depth = stack.depth.get() as usize;
                    let bytes = stack.elements[depth].bytes_total.get();
                    let count = stack.elements[depth].count_total.get();
                    let unsup_async = stack.elements[depth].unsupported_async.get();

                    stack.depth.set(stack.depth.get() - 1);

                    // If not in exclusive mode, accumulate to parent (cumulative mode)
                    if !super::shared::is_alloc_self_enabled() {
                        let parent = stack.depth.get() as usize;
                        stack.elements[parent]
                            .bytes_total
                            .set(stack.elements[parent].bytes_total.get() + bytes);
                        stack.elements[parent]
                            .count_total
                            .set(stack.elements[parent].count_total.get() + count);
                        stack.elements[parent]
                            .unsupported_async
                            .set(stack.elements[parent].unsupported_async.get() | unsup_async);
                    }

                    (bytes, count, unsup_async)
                })
            };

        // Temporarily disable allocation tracking to prevent measurement overhead
        // from being attributed to the parent function
        super::core::ALLOCATIONS.with(|stack| {
            stack.tracking_enabled.set(false);
        });

        super::state::send_alloc_measurement(
            self.name,
            bytes_total,
            count_total,
            unsupported_async,
            self.wrapper,
            cross_thread,
            self.tid,
        );

        // Re-enable allocation tracking
        super::core::ALLOCATIONS.with(|stack| {
            stack.tracking_enabled.set(true);
        });
    }
}
