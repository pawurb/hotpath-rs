//! Constants used across the TUI application

use std::time::Duration;

/// Timeout for HTTP requests to metrics server (milliseconds)
const HTTP_TIMEOUT_MS: u64 = 2000;

/// Polling interval for keyboard events in the main loop (milliseconds)
const EVENT_POLL_INTERVAL_MS: u64 = 100;

/// HTTP timeout as Duration
pub(crate) fn http_timeout() -> Duration {
    Duration::from_millis(HTTP_TIMEOUT_MS)
}

/// Event poll interval as Duration
pub(crate) fn event_poll_interval() -> Duration {
    Duration::from_millis(EVENT_POLL_INTERVAL_MS)
}
