#[cfg(feature = "dev")]
pub(crate) use tracing::{debug, error, info, trace, warn};

#[cfg(not(feature = "dev"))]
macro_rules! noop_log {
    ($($tt:tt)*) => {{
        let _ = format_args!($($tt)*);
    }};
}

#[cfg(not(feature = "dev"))]
pub(crate) use noop_log as debug;
#[cfg(not(feature = "dev"))]
pub(crate) use noop_log as error;
#[cfg(not(feature = "dev"))]
pub(crate) use noop_log as info;
#[cfg(not(feature = "dev"))]
pub(crate) use noop_log as trace;
#[cfg(not(feature = "dev"))]
pub(crate) use noop_log as warn;
