use crate::instant::Instant;

use crate::output::format_debug_truncated;

/// Wrapper guards are never sampled - their exact total is the `%` denominator.
/// Unsampled guards skip the start clock read and send a `None` duration;
/// elapsed time and result logs stay exact.
#[inline]
fn sampled_start(wrapper: bool, skipped: bool) -> Option<Instant> {
    if skipped {
        return None;
    }
    if wrapper || crate::lib_on::sampling::functions_should_time() {
        Some(Instant::now())
    } else {
        None
    }
}

#[doc(hidden)]
#[must_use = "guard is dropped immediately without measuring anything"]
pub struct MeasurementGuard {
    name: &'static str,
    start: Option<Instant>,
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
            start: sampled_start(wrapper, skipped),
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
        let duration_ns = self
            .start
            .map(|start| end.duration_since(start).as_nanos() as u64);
        let elapsed_since_start_ns = crate::lib_on::elapsed_since_start_ns(end);
        let cross_thread = crate::tid::current_tid() != self.tid;
        let tid = if cross_thread { None } else { Some(self.tid) };
        crate::lib_on::functions::timing::state::send_duration_measurement(
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
    start: Option<Instant>,
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
            start: sampled_start(wrapper, skipped),
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
        let duration_ns = self
            .start
            .map(|start| end.duration_since(start).as_nanos() as u64);
        let elapsed_since_start_ns = crate::lib_on::elapsed_since_start_ns(end);
        let result_str = Some(format_debug_truncated(result));
        let cross_thread = crate::tid::current_tid() != self.tid;
        let tid = if cross_thread { None } else { Some(self.tid) };
        crate::lib_on::functions::timing::state::send_duration_measurement_with_log(
            self.name,
            duration_ns,
            elapsed_since_start_ns,
            self.wrapper,
            tid,
            result_str,
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
        let duration_ns = self
            .start
            .map(|start| end.duration_since(start).as_nanos() as u64);
        let elapsed_since_start_ns = crate::lib_on::elapsed_since_start_ns(end);
        let cross_thread = crate::tid::current_tid() != self.tid;
        let tid = if cross_thread { None } else { Some(self.tid) };
        crate::lib_on::functions::timing::state::send_duration_measurement_with_log(
            self.name,
            duration_ns,
            elapsed_since_start_ns,
            self.wrapper,
            tid,
            None,
        );
    }
}
