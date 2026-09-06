//! Endpoint normalization: merges parameter-varied requests to the same route
//! into one bucket by collapsing identifier-like path segments into `{id}`.
//!
//! Input is the raw `METHOD host[:port]/path` pre-key built by the middleware
//! (query string, fragment, and credentials are already absent). Only path
//! segments are rewritten - the method and host prefix is left untouched, so
//! numeric hosts like `127.0.0.1:6770` survive.
//!
//! A segment is treated as an identifier when it is:
//! - all decimal digits (`/users/123`)
//! - a UUID (`/jobs/550e8400-e29b-41d4-a716-446655440000`)
//! - a hex string of 16+ chars (`/blobs/deadbeefdeadbeef`)

use regex_lite::Regex;
use std::sync::LazyLock;

static UUID_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$")
        .unwrap()
});

static HEX_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[0-9a-fA-F]{16,}$").unwrap());

fn is_id_segment(segment: &str) -> bool {
    !segment.is_empty()
        && (segment.bytes().all(|b| b.is_ascii_digit())
            || UUID_RE.is_match(segment)
            || HEX_RE.is_match(segment))
}

/// Normalize a raw `METHOD host[:port]/path` pre-key into a stable bucket key.
pub(crate) fn normalize_endpoint(endpoint: &str) -> String {
    let Some(slash) = endpoint.find('/') else {
        return endpoint.to_string();
    };
    let (prefix, path) = endpoint.split_at(slash);
    let normalized_path = path
        .split('/')
        .map(|segment| {
            if is_id_segment(segment) {
                "{id}"
            } else {
                segment
            }
        })
        .collect::<Vec<_>>()
        .join("/");
    format!("{prefix}{normalized_path}")
}
