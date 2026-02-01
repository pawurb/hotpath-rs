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
macro_rules! future {
    ($fut:expr) => {
        $fut
    };
    ($fut:expr, log = true) => {
        $fut
    };
}

pub use crate::Format;

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

pub struct FunctionsGuardBuilder {}

impl FunctionsGuardBuilder {
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

    pub fn output_path(self, _path: impl AsRef<std::path::Path>) -> Self {
        self
    }

    pub fn build(self) -> HotPath {
        HotPath
    }

    pub fn build_with_timeout(self, _duration: std::time::Duration) -> HotPath {
        HotPath
    }
}

#[derive(Debug, Clone)]
pub struct FunctionStats {}

pub mod channels {
    use super::Format;

    pub struct ChannelsGuardBuilder;

    impl ChannelsGuardBuilder {
        pub fn new() -> Self {
            Self
        }
        pub fn format(self, _format: Format) -> Self {
            self
        }
        pub fn output_path(self, _path: impl AsRef<std::path::Path>) -> Self {
            self
        }
        pub fn build(self) -> ChannelsGuard {
            ChannelsGuard
        }
    }

    impl Default for ChannelsGuardBuilder {
        fn default() -> Self {
            Self::new()
        }
    }

    pub struct ChannelsGuard;

    impl ChannelsGuard {
        pub fn new() -> Self {
            Self
        }
        pub fn format(self, _format: Format) -> Self {
            self
        }
        pub fn output_path(self, _path: impl AsRef<std::path::Path>) -> Self {
            self
        }
    }

    impl Default for ChannelsGuard {
        fn default() -> Self {
            Self::new()
        }
    }

    impl Drop for ChannelsGuard {
        fn drop(&mut self) {}
    }
}

pub mod streams {
    use super::Format;

    pub struct StreamsGuardBuilder;

    impl StreamsGuardBuilder {
        pub fn new() -> Self {
            Self
        }
        pub fn format(self, _format: Format) -> Self {
            self
        }
        pub fn output_path(self, _path: impl AsRef<std::path::Path>) -> Self {
            self
        }
        pub fn build(self) -> StreamsGuard {
            StreamsGuard
        }
    }

    impl Default for StreamsGuardBuilder {
        fn default() -> Self {
            Self::new()
        }
    }

    pub struct StreamsGuard;

    impl StreamsGuard {
        pub fn new() -> Self {
            Self
        }
        pub fn format(self, _format: Format) -> Self {
            self
        }
        pub fn output_path(self, _path: impl AsRef<std::path::Path>) -> Self {
            self
        }
    }

    impl Default for StreamsGuard {
        fn default() -> Self {
            Self::new()
        }
    }

    impl Drop for StreamsGuard {
        fn drop(&mut self) {}
    }
}

pub mod futures {
    use super::Format;

    pub struct FuturesGuardBuilder;

    impl FuturesGuardBuilder {
        pub fn new() -> Self {
            Self
        }
        pub fn format(self, _format: Format) -> Self {
            self
        }
        pub fn output_path(self, _path: impl AsRef<std::path::Path>) -> Self {
            self
        }
        pub fn build(self) -> FuturesGuard {
            FuturesGuard
        }
    }

    impl Default for FuturesGuardBuilder {
        fn default() -> Self {
            Self::new()
        }
    }

    pub struct FuturesGuard;

    impl FuturesGuard {
        pub fn new() -> Self {
            Self
        }
        pub fn format(self, _format: Format) -> Self {
            self
        }
        pub fn output_path(self, _path: impl AsRef<std::path::Path>) -> Self {
            self
        }
    }

    impl Default for FuturesGuard {
        fn default() -> Self {
            Self::new()
        }
    }

    impl Drop for FuturesGuard {
        fn drop(&mut self) {}
    }
}
