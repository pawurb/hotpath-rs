use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::sync::LazyLock;

#[cfg(target_os = "linux")]
use quanta::Instant;

#[cfg(not(target_os = "linux"))]
use std::time::Instant;

#[derive(Debug, Default)]
pub(crate) struct AsyncAllocBridge {
    bytes_total: AtomicU64,
    count_total: AtomicU64,
}

impl AsyncAllocBridge {
    #[inline]
    pub(crate) fn add(&self, bytes: u64, count: u64) {
        self.bytes_total.fetch_add(bytes, Ordering::Relaxed);
        self.count_total.fetch_add(count, Ordering::Relaxed);
    }

    #[inline]
    pub(crate) fn snapshot(&self) -> (Option<u64>, Option<u64>) {
        (
            Some(self.bytes_total.load(Ordering::Relaxed)),
            Some(self.count_total.load(Ordering::Relaxed)),
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AllocMetric {
    Bytes,
    Count,
}

pub(crate) static ALLOC_METRIC: LazyLock<AllocMetric> =
    LazyLock::new(|| match std::env::var("HOTPATH_ALLOC_METRIC") {
        Ok(v) => match v.to_lowercase().as_str() {
            "bytes" => AllocMetric::Bytes,
            "count" => AllocMetric::Count,
            other => panic!(
                "Invalid HOTPATH_ALLOC_METRIC value: '{}'. Expected 'bytes' or 'count'.",
                other
            ),
        },
        Err(_) => AllocMetric::Bytes,
    });

pub(crate) static ALLOC_SELF: LazyLock<bool> = LazyLock::new(|| {
    std::env::var("HOTPATH_ALLOC_SELF")
        .map(|v| v.eq_ignore_ascii_case("true") || v == "1")
        .unwrap_or(false)
});

#[inline]
pub(crate) fn push_alloc_stack() {
    crate::functions::alloc::core::ALLOCATIONS.with(|stack| {
        let current_depth = stack.depth.get();
        stack.depth.set(current_depth + 1);
        assert!((stack.depth.get() as usize) < crate::functions::alloc::core::MAX_DEPTH);
        let depth = stack.depth.get() as usize;
        stack.elements[depth].bytes_total.set(0);
        stack.elements[depth].count_total.set(0);
    });
}

#[inline]
pub(crate) fn pop_alloc_stack() -> (u64, u64) {
    crate::functions::alloc::core::ALLOCATIONS.with(|stack| {
        assert!(stack.depth.get() > 0, "pop_alloc_stack called with depth 0");
        let depth = stack.depth.get() as usize;
        let bytes = stack.elements[depth].bytes_total.get();
        let count = stack.elements[depth].count_total.get();

        stack.depth.set(stack.depth.get() - 1);

        if !*ALLOC_SELF {
            let parent = stack.depth.get() as usize;
            stack.elements[parent]
                .bytes_total
                .set(stack.elements[parent].bytes_total.get() + bytes);
            stack.elements[parent]
                .count_total
                .set(stack.elements[parent].count_total.get() + count);
        }

        (bytes, count)
    })
}

#[inline]
fn send_alloc_measurement(
    name: &'static str,
    bytes_total: Option<u64>,
    count_total: Option<u64>,
    duration: std::time::Duration,
    wrapper: bool,
    tid: Option<u64>,
) {
    crate::functions::alloc::core::ALLOCATIONS.with(|stack| {
        stack.tracking_enabled.set(false);
    });

    crate::functions::alloc::state::send_alloc_measurement(
        name,
        bytes_total,
        count_total,
        duration,
        wrapper,
        tid,
    );

    crate::functions::alloc::core::ALLOCATIONS.with(|stack| {
        stack.tracking_enabled.set(true);
    });
}

#[inline]
fn send_alloc_measurement_with_log(
    name: &'static str,
    bytes_total: Option<u64>,
    count_total: Option<u64>,
    duration: std::time::Duration,
    wrapper: bool,
    tid: Option<u64>,
    result_log: Option<String>,
) {
    crate::functions::alloc::core::ALLOCATIONS.with(|stack| {
        stack.tracking_enabled.set(false);
    });

    crate::functions::alloc::state::send_alloc_measurement_with_log(
        name,
        bytes_total,
        count_total,
        duration,
        wrapper,
        tid,
        result_log,
    );

    crate::functions::alloc::core::ALLOCATIONS.with(|stack| {
        stack.tracking_enabled.set(true);
    });
}

#[must_use = "guard is dropped immediately without measuring anything"]
pub struct MeasurementGuardSync {
    name: &'static str,
    wrapper: bool,
    tid: u64,
    start: Instant,
    skipped: bool,
}

impl MeasurementGuardSync {
    #[inline]
    pub fn new(name: &'static str, wrapper: bool, skipped: bool) -> Self {
        if !skipped {
            push_alloc_stack();
        }

        Self {
            name,
            wrapper,
            tid: crate::tid::current_tid(),
            start: Instant::now(),
            skipped,
        }
    }
}

impl Drop for MeasurementGuardSync {
    #[inline]
    fn drop(&mut self) {
        if self.skipped {
            return;
        }

        let duration = self.start.elapsed();
        let cross_thread = crate::tid::current_tid() != self.tid;

        let (bytes_total, count_total) = if cross_thread {
            (None, None)
        } else {
            let (bytes, count) = pop_alloc_stack();
            (Some(bytes), Some(count))
        };

        send_alloc_measurement(
            self.name,
            bytes_total,
            count_total,
            duration,
            self.wrapper,
            Some(self.tid),
        );
    }
}

#[must_use = "guard is dropped immediately without measuring anything"]
pub struct MeasurementGuardAsync {
    name: &'static str,
    wrapper: bool,
    tid: u64,
    start: Instant,
    skipped: bool,
    alloc_bridge: Option<Arc<AsyncAllocBridge>>,
}

impl MeasurementGuardAsync {
    #[inline]
    pub(crate) fn new(
        name: &'static str,
        wrapper: bool,
        skipped: bool,
        alloc_bridge: Option<Arc<AsyncAllocBridge>>,
    ) -> Self {
        Self {
            name,
            wrapper,
            tid: crate::tid::current_tid(),
            start: Instant::now(),
            skipped,
            alloc_bridge,
        }
    }
}

impl Drop for MeasurementGuardAsync {
    #[inline]
    fn drop(&mut self) {
        if self.skipped {
            return;
        }

        let duration = self.start.elapsed();
        let (bytes_total, count_total) = self
            .alloc_bridge
            .as_ref()
            .map_or((None, None), |bridge| bridge.snapshot());

        send_alloc_measurement(
            self.name,
            bytes_total,
            count_total,
            duration,
            self.wrapper,
            Some(self.tid),
        );
    }
}

#[must_use = "guard is dropped immediately without measuring anything"]
pub(crate) struct MeasurementGuardSyncWithLog {
    name: &'static str,
    wrapper: bool,
    tid: u64,
    start: Instant,
    finished: bool,
    skipped: bool,
}

impl MeasurementGuardSyncWithLog {
    #[inline]
    pub fn new(name: &'static str, wrapper: bool, skipped: bool) -> Self {
        if !skipped {
            push_alloc_stack();
        }

        Self {
            name,
            wrapper,
            tid: crate::tid::current_tid(),
            start: Instant::now(),
            finished: false,
            skipped,
        }
    }

    #[inline]
    pub fn finish_with_result<T: std::fmt::Debug>(mut self, result: &T) {
        self.finished = true;
        if self.skipped {
            return;
        }

        let duration = self.start.elapsed();
        let result_str = crate::output::format_debug_truncated(result);
        let cross_thread = crate::tid::current_tid() != self.tid;

        let (bytes_total, count_total) = if cross_thread {
            (None, None)
        } else {
            let (bytes, count) = pop_alloc_stack();
            (Some(bytes), Some(count))
        };

        send_alloc_measurement_with_log(
            self.name,
            bytes_total,
            count_total,
            duration,
            self.wrapper,
            Some(self.tid),
            Some(result_str),
        );
    }
}

impl Drop for MeasurementGuardSyncWithLog {
    #[inline]
    fn drop(&mut self) {
        if self.skipped || self.finished {
            return;
        }

        let duration = self.start.elapsed();
        let cross_thread = crate::tid::current_tid() != self.tid;

        let (bytes_total, count_total) = if cross_thread {
            (None, None)
        } else {
            let (bytes, count) = pop_alloc_stack();
            (Some(bytes), Some(count))
        };

        send_alloc_measurement_with_log(
            self.name,
            bytes_total,
            count_total,
            duration,
            self.wrapper,
            Some(self.tid),
            None,
        );
    }
}

#[must_use = "guard is dropped immediately without measuring anything"]
pub(crate) struct MeasurementGuardAsyncWithLog {
    name: &'static str,
    wrapper: bool,
    tid: u64,
    start: Instant,
    finished: bool,
    skipped: bool,
    alloc_bridge: Option<Arc<AsyncAllocBridge>>,
}

impl MeasurementGuardAsyncWithLog {
    #[inline]
    pub(crate) fn new(
        name: &'static str,
        wrapper: bool,
        skipped: bool,
        alloc_bridge: Option<Arc<AsyncAllocBridge>>,
    ) -> Self {
        Self {
            name,
            wrapper,
            tid: crate::tid::current_tid(),
            start: Instant::now(),
            finished: false,
            skipped,
            alloc_bridge,
        }
    }

    #[inline]
    pub fn finish_with_result<T: std::fmt::Debug>(mut self, result: &T) {
        self.finished = true;
        if self.skipped {
            return;
        }

        let duration = self.start.elapsed();
        let result_str = crate::output::format_debug_truncated(result);
        let (bytes_total, count_total) = self
            .alloc_bridge
            .as_ref()
            .map_or((None, None), |bridge| bridge.snapshot());

        send_alloc_measurement_with_log(
            self.name,
            bytes_total,
            count_total,
            duration,
            self.wrapper,
            Some(self.tid),
            Some(result_str),
        );
    }
}

impl Drop for MeasurementGuardAsyncWithLog {
    #[inline]
    fn drop(&mut self) {
        if self.skipped || self.finished {
            return;
        }

        let duration = self.start.elapsed();
        let (bytes_total, count_total) = self
            .alloc_bridge
            .as_ref()
            .map_or((None, None), |bridge| bridge.snapshot());

        send_alloc_measurement_with_log(
            self.name,
            bytes_total,
            count_total,
            duration,
            self.wrapper,
            Some(self.tid),
            None,
        );
    }
}
