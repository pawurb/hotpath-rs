use crate::instant::Instant;

use crate::output::format_debug_truncated;

#[doc(hidden)]
#[must_use = "guard is dropped immediately without measuring anything"]
pub struct MeasurementGuard {
    name: &'static str,
    start: Instant,
    wrapper: bool,
    tid: u64,
    skipped: bool,
}

#[cfg_attr(feature = "hotpath-meta", hotpath_meta::measure_all)]
impl MeasurementGuard {
    #[inline]
    pub fn new(name: &'static str, wrapper: bool, skipped: bool) -> Self {
        Self {
            name,
            start: Instant::now(),
            wrapper,
            tid: if skipped {
                0
            } else {
                crate::tid::current_tid()
            },
            skipped,
        }
    }
}

impl Drop for MeasurementGuard {
    #[inline]
    fn drop(&mut self) {
        if self.skipped {
            return;
        }
        let end = Instant::now();
        let duration_ns = end.duration_since(self.start).as_nanos() as u64;
        let elapsed_since_start_ns = crate::lib_on::START_TIME
            .get()
            .map(|start| end.duration_since(*start).as_nanos() as u64)
            .unwrap_or(0);
        let cross_thread = crate::tid::current_tid() != self.tid;
        let tid = if cross_thread { None } else { Some(self.tid) };
        super::state::send_duration_measurement(
            self.name,
            duration_ns,
            elapsed_since_start_ns,
            self.wrapper,
            tid,
        );
    }
}

#[doc(hidden)]
#[must_use = "guard is dropped immediately without measuring anything"]
pub(crate) struct MeasurementGuardWithLog {
    name: &'static str,
    start: Instant,
    wrapper: bool,
    tid: u64,
    finished: bool,
    skipped: bool,
}

#[cfg_attr(feature = "hotpath-meta", hotpath_meta::measure_all)]
impl MeasurementGuardWithLog {
    #[inline]
    pub fn new(name: &'static str, wrapper: bool, skipped: bool) -> Self {
        Self {
            name,
            start: Instant::now(),
            wrapper,
            tid: if skipped {
                0
            } else {
                crate::tid::current_tid()
            },
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
        let end = Instant::now();
        let duration_ns = end.duration_since(self.start).as_nanos() as u64;
        let elapsed_since_start_ns = crate::lib_on::START_TIME
            .get()
            .map(|start| end.duration_since(*start).as_nanos() as u64)
            .unwrap_or(0);
        let cross_thread = crate::tid::current_tid() != self.tid;
        let tid = if cross_thread { None } else { Some(self.tid) };
        let result_str = format_debug_truncated(result);
        super::state::send_duration_measurement_with_log(
            self.name,
            duration_ns,
            elapsed_since_start_ns,
            self.wrapper,
            tid,
            Some(result_str),
        );
    }
}

impl Drop for MeasurementGuardWithLog {
    #[inline]
    fn drop(&mut self) {
        if self.skipped || self.finished {
            return;
        }
        let end = Instant::now();
        let duration_ns = end.duration_since(self.start).as_nanos() as u64;
        let elapsed_since_start_ns = crate::lib_on::START_TIME
            .get()
            .map(|start| end.duration_since(*start).as_nanos() as u64)
            .unwrap_or(0);
        let cross_thread = crate::tid::current_tid() != self.tid;
        let tid = if cross_thread { None } else { Some(self.tid) };
        super::state::send_duration_measurement_with_log(
            self.name,
            duration_ns,
            elapsed_since_start_ns,
            self.wrapper,
            tid,
            None,
        );
    }
}
