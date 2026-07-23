//! Query normalization: merges parameter-varied executions of the same
//! statement into one bucket using cheap regex substitutions (no SQL parser).
//!
//! Transformations, applied in order:
//! - single-quoted string literals -> `?`
//! - PostgreSQL positional placeholders (`$1`, `$2`, ...) -> `?`
//! - SQLite numbered placeholders (`?1`, `?2`, ...) -> `?`
//! - numeric literals -> `?`
//! - runs of `?` inside an `IN (...)` list -> `IN (?)`
//! - collapse all whitespace to single spaces

use regex::Regex;
use std::sync::LazyLock;

// Single-quoted literal, with '' as an escaped quote inside.
static STRING_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"'(?:[^']|'')*'").unwrap());

static PG_PLACEHOLDER_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\$\d+\b").unwrap());

// Must run before NUMBER_RE, or the digit alone would be replaced and `?1`
// would come out as `??`.
static NUMBERED_PLACEHOLDER_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\?\d+\b").unwrap());

// Standalone numeric literals (int/float), not parts of identifiers.
static NUMBER_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\b\d+(?:\.\d+)?\b").unwrap());

static IN_LIST_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\bIN\s*\(\s*\?(?:\s*,\s*\?)*\s*\)").unwrap());

static WHITESPACE_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s+").unwrap());

/// Normalize a raw SQL string into a stable bucket key.
pub(crate) fn normalize(sql: &str) -> String {
    let s = STRING_RE.replace_all(sql, "?");
    let s = PG_PLACEHOLDER_RE.replace_all(&s, "?");
    let s = NUMBERED_PLACEHOLDER_RE.replace_all(&s, "?");
    let s = NUMBER_RE.replace_all(&s, "?");
    let s = IN_LIST_RE.replace_all(&s, "IN (?)");
    let s = WHITESPACE_RE.replace_all(&s, " ");
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
    fn merges_numbered_placeholders() {
        // SQLite-style numbered placeholders collapse to plain `?`, matching
        // the PostgreSQL `$N` handling.
        assert_eq!(
            normalize("INSERT INTO users (name, age) VALUES (?1, ?2)"),
            "INSERT INTO users (name, age) VALUES (?, ?)",
        );
        assert_eq!(
            normalize("SELECT * FROM users WHERE id = ?1"),
            normalize("SELECT * FROM users WHERE id = $1"),
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
    fn merges_pg_placeholders() {
        assert_eq!(
            normalize("INSERT INTO users (name, age) VALUES ($1, $2)"),
            "INSERT INTO users (name, age) VALUES (?, ?)",
        );
        assert_eq!(
            normalize("SELECT * FROM t WHERE id IN ($1, $2, $3)"),
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
