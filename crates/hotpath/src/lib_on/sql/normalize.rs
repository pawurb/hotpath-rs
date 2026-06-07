//! Query normalization: merges parameter-varied executions of the same
//! statement into one bucket using cheap regex substitutions (no SQL parser).
//!
//! Transformations, applied in order:
//! - single-quoted string literals -> `?`
//! - numeric literals -> `?`
//! - runs of `?` inside an `IN (...)` list -> `IN (?)`
//! - collapse all whitespace to single spaces

use regex::Regex;
use std::sync::OnceLock;

fn string_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    // Single-quoted literal, with '' as an escaped quote inside.
    RE.get_or_init(|| Regex::new(r"'(?:[^']|'')*'").unwrap())
}

fn number_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    // Standalone numeric literals (int/float), not parts of identifiers.
    RE.get_or_init(|| Regex::new(r"\b\d+(?:\.\d+)?\b").unwrap())
}

fn in_list_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)\bIN\s*\(\s*\?(?:\s*,\s*\?)*\s*\)").unwrap())
}

fn whitespace_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\s+").unwrap())
}

/// Normalize a raw SQL string into a stable bucket key.
pub(crate) fn normalize(sql: &str) -> String {
    let s = string_re().replace_all(sql, "?");
    let s = number_re().replace_all(&s, "?");
    let s = in_list_re().replace_all(&s, "IN (?)");
    let s = whitespace_re().replace_all(&s, " ");
    s.trim().to_string()
}

#[cfg(test)]
mod tests {
    use crate::lib_on::sql::normalize::normalize;

    #[test]
    fn merges_integer_literals() {
        assert_eq!(
            normalize("SELECT * FROM users WHERE id = 1"),
            normalize("SELECT * FROM users WHERE id = 42"),
        );
        assert_eq!(
            normalize("SELECT * FROM users WHERE id = 1"),
            "SELECT * FROM users WHERE id = ?",
        );
    }

    #[test]
    fn merges_string_literals() {
        assert_eq!(
            normalize("SELECT * FROM t WHERE name = 'alice'"),
            normalize("SELECT * FROM t WHERE name = 'bob'"),
        );
    }

    #[test]
    fn collapses_in_lists() {
        assert_eq!(
            normalize("SELECT * FROM t WHERE id IN (1, 2, 3)"),
            normalize("SELECT * FROM t WHERE id IN (9)"),
        );
        assert_eq!(
            normalize("SELECT * FROM t WHERE id IN (1, 2, 3)"),
            "SELECT * FROM t WHERE id IN (?)",
        );
    }

    #[test]
    fn collapses_placeholder_in_lists() {
        assert_eq!(
            normalize("SELECT * FROM t WHERE id IN (?, ?, ?)"),
            "SELECT * FROM t WHERE id IN (?)",
        );
    }

    #[test]
    fn squashes_whitespace() {
        assert_eq!(normalize("SELECT   *\n  FROM   t"), "SELECT * FROM t",);
    }
}
