#[cfg(target_os = "linux")]
use quanta::Instant;

#[cfg(not(target_os = "linux"))]
use std::time::Instant;

use super::super::truncate_result;

#[doc(hidden)]
pub struct MeasurementGuard {
    name: &'static str,
    start: Instant,
    wrapper: bool,
    tid: u64,
}

impl MeasurementGuard {
    #[inline]
    pub fn new(name: &'static str, wrapper: bool, _unsupported_sync: bool) -> Self {
        Self {
            name,
            start: Instant::now(),
            wrapper,
            tid: crate::tid::current_tid(),
        }
    }
}

impl Drop for MeasurementGuard {
    #[inline]
    fn drop(&mut self) {
        let dur = self.start.elapsed();
        let cross_thread = crate::tid::current_tid() != self.tid;
        let tid = if cross_thread { None } else { Some(self.tid) };
        super::state::send_duration_measurement(self.name, dur, self.wrapper, tid);
    }
}

#[doc(hidden)]
pub struct MeasurementGuardWithLog {
    name: &'static str,
    start: Instant,
    wrapper: bool,
    tid: u64,
    finished: bool,
}

impl MeasurementGuardWithLog {
    #[inline]
    pub fn new(name: &'static str, wrapper: bool, _unsupported_sync: bool) -> Self {
        Self {
            name,
            start: Instant::now(),
            wrapper,
            tid: crate::tid::current_tid(),
            finished: false,
        }
    }

    #[inline]
    pub fn finish_with_result<T: std::fmt::Debug>(mut self, result: &T) {
        self.finished = true;
        let dur = self.start.elapsed();
        let cross_thread = crate::tid::current_tid() != self.tid;
        let tid = if cross_thread { None } else { Some(self.tid) };
        let result_str = truncate_result(format!("{:?}", result));
        super::state::send_duration_measurement_with_log(
            self.name,
            dur,
            self.wrapper,
            tid,
            Some(result_str),
        );
    }
}

impl Drop for MeasurementGuardWithLog {
    #[inline]
    fn drop(&mut self) {
        if !self.finished {
            let dur = self.start.elapsed();
            let cross_thread = crate::tid::current_tid() != self.tid;
            let tid = if cross_thread { None } else { Some(self.tid) };
            super::state::send_duration_measurement_with_log(
                self.name,
                dur,
                self.wrapper,
                tid,
                None,
            );
        }
    }
}
