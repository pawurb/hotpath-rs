#[cfg(target_os = "linux")]
use quanta::Instant;

#[cfg(not(target_os = "linux"))]
use std::time::Instant;

#[must_use = "guard is dropped immediately without measuring anything"]
pub struct MeasurementGuard {
    name: &'static str,
    wrapper: bool,
    tid: u64,
    start: Instant,
    skipped: bool,
    is_async: bool,
}

impl MeasurementGuard {
    #[inline]
    pub fn new(name: &'static str, wrapper: bool, skipped: bool, is_async: bool) -> Self {
        if !skipped && !is_async {
            super::core::ALLOCATIONS.with(|stack| {
                let current_depth = stack.depth.get();
                stack.depth.set(current_depth + 1);
                assert!((stack.depth.get() as usize) < super::core::MAX_DEPTH);
                let depth = stack.depth.get() as usize;
                stack.elements[depth].bytes_total.set(0);
                stack.elements[depth].count_total.set(0);
            });
        }

        Self {
            name,
            wrapper,
            tid: crate::tid::current_tid(),
            start: Instant::now(),
            skipped,
            is_async,
        }
    }
}

impl Drop for MeasurementGuard {
    #[inline]
    fn drop(&mut self) {
        if self.skipped {
            return;
        }

        let duration = self.start.elapsed();
        let cross_thread = crate::tid::current_tid() != self.tid;

        let (bytes_total, count_total) = if self.is_async || cross_thread {
            (None, None)
        } else {
            super::core::ALLOCATIONS.with(|stack| {
                let depth = stack.depth.get() as usize;
                let bytes = stack.elements[depth].bytes_total.get();
                let count = stack.elements[depth].count_total.get();

                stack.depth.set(stack.depth.get() - 1);

                if !super::shared::is_alloc_self_enabled() {
                    let parent = stack.depth.get() as usize;
                    stack.elements[parent]
                        .bytes_total
                        .set(stack.elements[parent].bytes_total.get() + bytes);
                    stack.elements[parent]
                        .count_total
                        .set(stack.elements[parent].count_total.get() + count);
                }

                (Some(bytes), Some(count))
            })
        };

        super::core::ALLOCATIONS.with(|stack| {
            stack.tracking_enabled.set(false);
        });

        super::state::send_alloc_measurement(
            self.name,
            bytes_total,
            count_total,
            duration,
            self.wrapper,
            Some(self.tid),
        );

        super::core::ALLOCATIONS.with(|stack| {
            stack.tracking_enabled.set(true);
        });
    }
}

#[must_use = "guard is dropped immediately without measuring anything"]
pub struct MeasurementGuardWithLog {
    name: &'static str,
    wrapper: bool,
    tid: u64,
    start: Instant,
    finished: bool,
    skipped: bool,
    is_async: bool,
}

impl MeasurementGuardWithLog {
    #[inline]
    pub fn new(name: &'static str, wrapper: bool, skipped: bool, is_async: bool) -> Self {
        if !skipped && !is_async {
            super::core::ALLOCATIONS.with(|stack| {
                let current_depth = stack.depth.get();
                stack.depth.set(current_depth + 1);
                assert!((stack.depth.get() as usize) < super::core::MAX_DEPTH);
                let depth = stack.depth.get() as usize;
                stack.elements[depth].bytes_total.set(0);
                stack.elements[depth].count_total.set(0);
            });
        }

        Self {
            name,
            wrapper,
            tid: crate::tid::current_tid(),
            start: Instant::now(),
            finished: false,
            skipped,
            is_async,
        }
    }

    #[inline]
    pub fn finish_with_result<T: std::fmt::Debug>(mut self, result: &T) {
        self.finished = true;
        if self.skipped {
            return;
        }
        let result_str = crate::output::format_debug_truncated(result);

        let duration = self.start.elapsed();
        let cross_thread = crate::tid::current_tid() != self.tid;

        let (bytes_total, count_total) = if self.is_async || cross_thread {
            (None, None)
        } else {
            super::core::ALLOCATIONS.with(|stack| {
                let depth = stack.depth.get() as usize;
                let bytes = stack.elements[depth].bytes_total.get();
                let count = stack.elements[depth].count_total.get();

                stack.depth.set(stack.depth.get() - 1);

                if !super::shared::is_alloc_self_enabled() {
                    let parent = stack.depth.get() as usize;
                    stack.elements[parent]
                        .bytes_total
                        .set(stack.elements[parent].bytes_total.get() + bytes);
                    stack.elements[parent]
                        .count_total
                        .set(stack.elements[parent].count_total.get() + count);
                }

                (Some(bytes), Some(count))
            })
        };

        super::core::ALLOCATIONS.with(|stack| {
            stack.tracking_enabled.set(false);
        });

        super::state::send_alloc_measurement_with_log(
            self.name,
            bytes_total,
            count_total,
            duration,
            self.wrapper,
            Some(self.tid),
            Some(result_str),
        );

        super::core::ALLOCATIONS.with(|stack| {
            stack.tracking_enabled.set(true);
        });
    }
}

impl Drop for MeasurementGuardWithLog {
    #[inline]
    fn drop(&mut self) {
        if self.skipped || self.finished {
            return;
        }

        let duration = self.start.elapsed();
        let cross_thread = crate::tid::current_tid() != self.tid;

        let (bytes_total, count_total) = if self.is_async || cross_thread {
            (None, None)
        } else {
            super::core::ALLOCATIONS.with(|stack| {
                let depth = stack.depth.get() as usize;
                let bytes = stack.elements[depth].bytes_total.get();
                let count = stack.elements[depth].count_total.get();

                stack.depth.set(stack.depth.get() - 1);

                if !super::shared::is_alloc_self_enabled() {
                    let parent = stack.depth.get() as usize;
                    stack.elements[parent]
                        .bytes_total
                        .set(stack.elements[parent].bytes_total.get() + bytes);
                    stack.elements[parent]
                        .count_total
                        .set(stack.elements[parent].count_total.get() + count);
                }

                (Some(bytes), Some(count))
            })
        };

        super::core::ALLOCATIONS.with(|stack| {
            stack.tracking_enabled.set(false);
        });

        super::state::send_alloc_measurement_with_log(
            self.name,
            bytes_total,
            count_total,
            duration,
            self.wrapper,
            Some(self.tid),
            None,
        );

        super::core::ALLOCATIONS.with(|stack| {
            stack.tracking_enabled.set(true);
        });
    }
}
