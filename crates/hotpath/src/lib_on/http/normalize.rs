//! Endpoint normalization: merges parameter-varied requests to the same route
//! into one bucket by collapsing identifier-like path segments into `{id}`.
//!
//! Input is the raw `METHOD host[:port]/path` pre-key built by the middleware
//! (query string, fragment, and credentials are already absent). The method
//! and host are left untouched, so numeric hosts like `127.0.0.1` survive; an
//! explicit port becomes `{port}`, since a port that is not the scheme's
//! default is most often a test server bound to a free one, and two runs
//! must key the same endpoint the same way to be comparable.
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
        return normalize_port(endpoint);
    };
    let (prefix, path) = endpoint.split_at(slash);
    let prefix = normalize_port(prefix);
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

/// `METHOD host:1234` -> `METHOD host:{port}`; a prefix without a port, or
/// whose last `:` is not followed by digits (an IPv6 literal's last group),
/// is returned as is.
fn normalize_port(prefix: &str) -> String {
    match prefix.rsplit_once(':') {
        Some((host, port)) if !port.is_empty() && port.bytes().all(|b| b.is_ascii_digit()) => {
            format!("{host}:{{port}}")
        }
        _ => prefix.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use crate::lib_on::http::normalize::normalize_endpoint;

    #[test]
    fn merges_numeric_segments() {
        assert_eq!(
            normalize_endpoint("GET api.example.com/users/1"),
            normalize_endpoint("GET api.example.com/users/42"),
        );
        assert_eq!(
            normalize_endpoint("GET api.example.com/users/1"),
            "GET api.example.com/users/{id}",
        );
    }

    #[test]
    fn merges_uuid_segments() {
        assert_eq!(
            normalize_endpoint("GET api.example.com/jobs/550e8400-e29b-41d4-a716-446655440000"),
            "GET api.example.com/jobs/{id}",
        );
    }

    #[test]
    fn merges_long_hex_segments() {
        assert_eq!(
            normalize_endpoint("GET api.example.com/blobs/deadbeefdeadbeef"),
            "GET api.example.com/blobs/{id}",
        );
    }

    #[test]
    fn keeps_short_hex_and_words() {
        assert_eq!(
            normalize_endpoint("GET api.example.com/cafe/abc123x"),
            "GET api.example.com/cafe/abc123x",
        );
    }

    /// A host survives as written; an explicit port is a parameter, so a
    /// test server bound to a free port keys the same across runs.
    #[test]
    fn keeps_numeric_host_and_merges_ports() {
        assert_eq!(
            normalize_endpoint("GET 127.0.0.1:6770/users/5"),
            "GET 127.0.0.1:{port}/users/{id}",
        );
        assert_eq!(
            normalize_endpoint("GET 127.0.0.1:6770/users/5"),
            normalize_endpoint("GET 127.0.0.1:38607/users/9"),
        );
        assert_eq!(
            normalize_endpoint("GET api.example.com:8443/health"),
            "GET api.example.com:{port}/health",
        );
        assert_eq!(
            normalize_endpoint("GET api.example.com/health"),
            "GET api.example.com/health",
        );
        // an IPv6 literal without a port keeps its last group
        assert_eq!(normalize_endpoint("GET [::1]/health"), "GET [::1]/health",);
    }

    #[test]
    fn nested_ids() {
        assert_eq!(
            normalize_endpoint("POST api.example.com/users/12/posts/34/comments"),
            "POST api.example.com/users/{id}/posts/{id}/comments",
        );
    }

    #[test]
    fn root_path() {
        assert_eq!(
            normalize_endpoint("GET api.example.com/"),
            "GET api.example.com/",
        );
    }

    #[test]
    fn no_path_passthrough() {
        assert_eq!(normalize_endpoint("GET nowhere"), "GET nowhere");
        assert_eq!(normalize_endpoint("GET nowhere:80"), "GET nowhere:{port}");
    }
}
