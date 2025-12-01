pub use cfg_if::cfg_if;
pub use hotpath_macros::{main, measure, measure_all, skip};

#[macro_export]
macro_rules! measure_block {
    ($label:expr, $expr:expr) => {{
        $expr
    }};
}

/// No-op channel macro when hotpath is disabled
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

/// No-op stream macro when hotpath is disabled
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

#[derive(Clone, Copy, Debug, Default)]
pub enum Format {
    #[default]
    Table,
    Json,
    JsonPretty,
}

pub struct MeasurementGuard {}

impl MeasurementGuard {
    pub fn new(_name: &'static str, _wrapper: bool, _unsupported_async: bool) -> Self {
        Self {}
    }

    pub fn build(_name: &'static str, _wrapper: bool, _is_async: bool) -> Self {
        Self {}
    }

    pub fn build_with_timeout(self, _duration: std::time::Duration) {}
}

pub struct MeasurementGuardWithLog {}

impl MeasurementGuardWithLog {
    pub fn new(_name: &'static str, _wrapper: bool, _unsupported_async: bool) -> Self {
        Self {}
    }

    pub fn build(_name: &'static str, _wrapper: bool, _is_async: bool) -> Self {
        Self {}
    }

    pub fn finish_with_result<T: std::fmt::Debug>(self, _result: &T) {}
}

#[inline]
pub fn measure_with_log<T: std::fmt::Debug, F: FnOnce() -> T>(
    _name: &'static str,
    _wrapper: bool,
    _is_async: bool,
    f: F,
) -> T {
    f()
}

pub async fn measure_with_log_async<T: std::fmt::Debug, F, Fut>(_name: &'static str, f: F) -> T
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = T>,
{
    f().await
}

pub struct HotPath;

impl Default for HotPath {
    fn default() -> Self {
        Self::new()
    }
}

impl HotPath {
    pub fn new() -> Self {
        Self {}
    }
}

pub struct GuardBuilder {}
use crate::Reporter;

impl GuardBuilder {
    pub fn new(_caller_name: impl Into<String>) -> Self {
        Self {}
    }

    pub fn percentiles(self, _percentiles: &[u8]) -> Self {
        self
    }

    pub fn format(self, _format: Format) -> Self {
        self
    }

    pub fn limit(self, _limit: usize) -> Self {
        self
    }

    pub fn build(self) -> HotPath {
        HotPath
    }

    pub fn build_with_timeout(self, _duration: std::time::Duration) -> HotPath {
        HotPath
    }

    pub fn reporter(self, _reporter: Box<dyn Reporter>) -> Self {
        self
    }
}

#[derive(Debug, Clone)]
pub struct FunctionStats {}
