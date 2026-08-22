//! Shared-secret token check used by the metrics and MCP servers. The
//! `Authorization` header value must equal the token exactly, no scheme
//! prefix handling.

/// Panics on whitespace, control or non-ASCII characters - HTTP clients would
/// trim, reject or mangle them, so auth would silently fail.
pub(crate) fn token_from_env(var: &str) -> Option<String> {
    let token = std::env::var(var).ok().filter(|s| !s.is_empty())?;
    assert!(
        token.bytes().all(|b| b.is_ascii_graphic()),
        "{var} must contain only printable ASCII characters without whitespace"
    );
    Some(token)
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter()
        .zip(b.iter())
        .fold(0u8, |acc, (x, y)| acc | (x ^ y))
        == 0
}

pub(crate) fn check_auth(expected: Option<&str>, provided: Option<&str>) -> bool {
    match expected {
        None => true,
        Some(expected) => provided
            .map(|p| constant_time_eq(p.as_bytes(), expected.as_bytes()))
            .unwrap_or(false),
    }
}

#[cfg(test)]
mod tests {
    use crate::auth::{check_auth, token_from_env};

    #[test]
    fn auth_disabled_allows_all() {
        assert!(check_auth(None, None));
        assert!(check_auth(None, Some("anything")));
    }

    #[test]
    fn auth_enabled_rejects_missing() {
        assert!(!check_auth(Some("secret"), None));
    }

    #[test]
    fn auth_enabled_rejects_wrong() {
        assert!(!check_auth(Some("secret"), Some("wrong")));
        assert!(!check_auth(Some("secret"), Some("Secret")));
        assert!(!check_auth(Some("secret"), Some("")));
    }

    #[test]
    fn auth_enabled_accepts_correct() {
        assert!(check_auth(Some("secret"), Some("secret")));
        assert!(check_auth(Some("Bearer token"), Some("Bearer token")));
    }

    #[test]
    fn token_from_env_filters_empty() {
        let var = "HOTPATH_AUTH_TEST_TOKEN";
        std::env::remove_var(var);
        assert_eq!(token_from_env(var), None);
        std::env::set_var(var, "");
        assert_eq!(token_from_env(var), None);
        std::env::set_var(var, "YWJj+/=!#$%");
        assert_eq!(token_from_env(var), Some("YWJj+/=!#$%".to_string()));
        std::env::remove_var(var);
    }

    #[test]
    #[should_panic(expected = "must contain only printable ASCII")]
    fn token_from_env_panics_on_invalid_chars() {
        let var = "HOTPATH_AUTH_TEST_INVALID";
        std::env::set_var(var, "abc def");
        let _ = token_from_env(var);
    }
}
