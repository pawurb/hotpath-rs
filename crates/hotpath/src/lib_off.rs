pub use hotpath_macros::{future_fn, main, measure, measure_all, skip};

#[macro_export]
macro_rules! measure_block {
    ($label:expr, $expr:expr) => {{
        $expr
    }};
}

#[macro_export]
macro_rules! dbg {
    ($val:expr $(,)?) => {
        match $val {
            tmp => tmp
        }
    };
    ($($val:expr),+ $(,)?) => {
        ($($crate::dbg!($val)),+,)
    };
}

#[macro_export]
macro_rules! val {
    ($key:expr) => {{
        $crate::ValHandle
    }};
}

pub struct ValHandle;

impl ValHandle {
    #[inline]
    pub fn set<T: std::fmt::Debug>(&self, _value: &T) {}
}

pub use crate::shared::IntoF64;

#[macro_export]
macro_rules! gauge {
    ($key:expr) => {{
        $crate::GaugeHandle
    }};
}

pub struct GaugeHandle;

impl GaugeHandle {
    #[inline]
    pub fn set(&self, _value: impl IntoF64) -> &Self {
        self
    }

    #[inline]
    pub fn inc(&self, _delta: impl IntoF64) -> &Self {
        self
    }

    #[inline]
    pub fn dec(&self, _delta: impl IntoF64) -> &Self {
        self
    }
}

#[macro_export]
macro_rules! channel {
    ($expr:expr) => {
        $expr
    };
    ($expr:expr, label = $label:expr) => {
        $expr
    };
    ($expr:expr, capacity = $capacity:expr) => {
        $expr
    };
    ($expr:expr, label = $label:expr, capacity = $capacity:expr) => {
        $expr
    };
    ($expr:expr, capacity = $capacity:expr, label = $label:expr) => {
        $expr
    };
    ($expr:expr, log = true) => {
        $expr
    };
    ($expr:expr, label = $label:expr, log = true) => {
        $expr
    };
    ($expr:expr, log = true, label = $label:expr) => {
        $expr
    };
    ($expr:expr, capacity = $capacity:expr, log = true) => {
        $expr
    };
    ($expr:expr, log = true, capacity = $capacity:expr) => {
        $expr
    };
    ($expr:expr, label = $label:expr, capacity = $capacity:expr, log = true) => {
        $expr
    };
    ($expr:expr, label = $label:expr, log = true, capacity = $capacity:expr) => {
        $expr
    };
    ($expr:expr, capacity = $capacity:expr, label = $label:expr, log = true) => {
        $expr
    };
    ($expr:expr, capacity = $capacity:expr, log = true, label = $label:expr) => {
        $expr
    };
    ($expr:expr, log = true, label = $label:expr, capacity = $capacity:expr) => {
        $expr
    };
    ($expr:expr, log = true, capacity = $capacity:expr, label = $label:expr) => {
        $expr
    };
}

#[macro_export]
macro_rules! stream {
    ($expr:expr) => {
        $expr
    };
    ($expr:expr, label = $label:expr) => {
        $expr
    };
    ($expr:expr, log = true) => {
        $expr
    };
    ($expr:expr, label = $label:expr, log = true) => {
        $expr
    };
    ($expr:expr, log = true, label = $label:expr) => {
        $expr
    };
}

#[macro_export]
macro_rules! tokio_runtime {
    () => {};
    ($handle:expr) => {};
}

#[macro_export]
macro_rules! future {
    ($fut:expr) => {
        $fut
    };
    ($fut:expr, label = $label:expr) => {
        $fut
    };
    ($fut:expr, log = true) => {
        $fut
    };
    ($fut:expr, label = $label:expr, log = true) => {
        $fut
    };
    ($fut:expr, log = true, label = $label:expr) => {
        $fut
    };
}

pub use crate::Format;
pub use crate::Section;

pub struct MeasurementGuard {}

impl MeasurementGuard {
    pub fn new(_name: &'static str, _wrapper: bool) -> Self {
        Self {}
    }

    pub fn build(_name: &'static str, _wrapper: bool) -> Self {
        Self {}
    }
}

#[inline]
pub fn measure_sync_log<T: std::fmt::Debug, F: FnOnce() -> T>(
    _measurement_loc: &'static str,
    f: F,
) -> T {
    f()
}

pub async fn measure_async<T, Fut>(_measurement_loc: &'static str, fut: Fut) -> T
where
    Fut: std::future::Future<Output = T>,
{
    fut.await
}

pub async fn measure_async_log<T: std::fmt::Debug, Fut>(
    _measurement_loc: &'static str,
    fut: Fut,
) -> T
where
    Fut: std::future::Future<Output = T>,
{
    fut.await
}

pub async fn measure_async_future<T, Fut>(_measurement_loc: &'static str, fut: Fut) -> T
where
    Fut: std::future::Future<Output = T>,
{
    fut.await
}

pub async fn measure_async_future_log<T: std::fmt::Debug, Fut>(
    _measurement_loc: &'static str,
    fut: Fut,
) -> T
where
    Fut: std::future::Future<Output = T>,
{
    fut.await
}

pub struct HotpathGuard;

impl Default for HotpathGuard {
    fn default() -> Self {
        Self::new()
    }
}

impl HotpathGuard {
    pub(crate) fn new() -> Self {
        Self {}
    }
}

pub struct HotpathGuardBuilder {}

impl HotpathGuardBuilder {
    pub fn new(_caller_name: &'static str) -> Self {
        Self {}
    }

    pub fn percentiles(self, _percentiles: &[f64]) -> Self {
        self
    }

    pub fn format(self, _format: Format) -> Self {
        self
    }

    pub fn functions_limit(self, _limit: usize) -> Self {
        self
    }

    pub fn channels_limit(self, _limit: usize) -> Self {
        self
    }

    pub fn streams_limit(self, _limit: usize) -> Self {
        self
    }

    pub fn futures_limit(self, _limit: usize) -> Self {
        self
    }

    pub fn threads_limit(self, _limit: usize) -> Self {
        self
    }

    pub fn rw_locks_limit(self, _limit: usize) -> Self {
        self
    }

    pub fn limit(self, _limit: usize) -> Self {
        self
    }

    pub fn output_path(self, _path: impl AsRef<std::path::Path>) -> Self {
        self
    }

    pub fn sections(self, _sections: Vec<Section>) -> Self {
        self
    }

    pub fn before_shutdown(self, _f: impl FnOnce() + Send + 'static) -> Self {
        self
    }

    pub fn build(self) -> HotpathGuard {
        HotpathGuard
    }

    pub fn build_with_shutdown(self, _duration: std::time::Duration) {}
}

pub mod channels {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum ChannelType {
        Bounded(usize),
        Unbounded,
        Oneshot,
    }
}

pub mod streams {}

pub mod threads {}

pub mod futures {}

pub mod rw_locks {
    pub use std::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard};
}

#[cfg(feature = "parking_lot")]
pub mod parking_lot {
    pub use parking_lot::{RwLock, RwLockReadGuard, RwLockWriteGuard};
}

#[macro_export]
macro_rules! rw_lock {
    ($expr:expr) => {
        $expr
    };
    ($expr:expr, label = $label:expr) => {
        $expr
    };
}
