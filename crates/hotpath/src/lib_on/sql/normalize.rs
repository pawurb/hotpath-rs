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

use regex_lite::Regex;
use std::sync::LazyLock;

// Single-quoted literal, with '' as an escaped quote inside.
static STRING_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"'(?:[^']|'')*'").unwrap());

static PG_PLACEHOLDER_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\$\d+\b").unwrap());

// Must run before NUMBER_RE, or the digit alone would be replaced and `?1`
// would come out as `??`.
static NUMBERED_PLACEHOLDER_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\?\d+\b").unwrap());

// Numeric candidates (int/float); identifier boundaries are checked in
// `replace_numbers` because regex-lite's `\b` is ASCII-only and would split
// `café1` into `café` + `1`.
static NUMBER_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\d+(?:\.\d+)?").unwrap());

static IN_LIST_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\bIN\s*\(\s*\?(?:\s*,\s*\?)*\s*\)").unwrap());

static WHITESPACE_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s+").unwrap());

/// Normalize a raw SQL string into a stable bucket key.
pub(crate) fn normalize(sql: &str) -> String {
    let s = STRING_RE.replace_all(sql, "?");
    let s = PG_PLACEHOLDER_RE.replace_all(&s, "?");
    let s = NUMBERED_PLACEHOLDER_RE.replace_all(&s, "?");
    let s = replace_numbers(&s);
    let s = IN_LIST_RE.replace_all(&s, "IN (?)");
    let s = WHITESPACE_RE.replace_all(&s, " ");
    s.trim().to_string()
}

// Every non-ASCII char counts as identifier content: string literals are
// already replaced by this point and all SQL punctuation that can sit next to
// a numeric literal is ASCII. This also covers combining marks, which
// `char::is_alphanumeric` rejects.
fn is_ident_char(c: char) -> bool {
    !c.is_ascii() || c.is_ascii_alphanumeric() || c == '_'
}

/// Replaces standalone numeric literals with `?`, leaving digits that are part
/// of an identifier (`t1`, `2fa`, `café1`) untouched.
fn replace_numbers(sql: &str) -> String {
    let mut out = String::with_capacity(sql.len());
    let mut last = 0;
    for m in NUMBER_RE.find_iter(sql) {
        let before = sql[..m.start()].chars().next_back();
        let after = sql[m.end()..].chars().next();
        let inside_ident = before.is_some_and(is_ident_char) || after.is_some_and(is_ident_char);
        out.push_str(&sql[last..m.start()]);
        out.push_str(if inside_ident { m.as_str() } else { "?" });
        last = m.end();
    }
    out.push_str(&sql[last..]);
    out
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
    fn keeps_digits_inside_identifiers() {
        assert_eq!(
            normalize("SELECT t1.x FROM t1 WHERE id = 10"),
            "SELECT t1.x FROM t1 WHERE id = ?",
        );
        assert_eq!(normalize("SELECT * FROM 2fa"), "SELECT * FROM 2fa");
    }

    #[test]
    fn keeps_digits_inside_unicode_identifiers() {
        assert_eq!(normalize("SELECT café1 FROM t"), "SELECT café1 FROM t");
        assert_ne!(
            normalize("SELECT café1 FROM t"),
            normalize("SELECT café2 FROM t"),
        );
    }

    #[test]
    fn keeps_digits_after_combining_marks() {
        // Decomposed form: `e` followed by U+0301 COMBINING ACUTE ACCENT.
        let decomposed = "SELECT \"cafe\u{301}1\" FROM t";
        assert_eq!(normalize(decomposed), decomposed);
        assert_ne!(
            normalize(decomposed),
            normalize("SELECT \"cafe\u{301}2\" FROM t"),
        );
    }

    #[test]
    fn squashes_whitespace() {
        assert_eq!(normalize("SELECT   *\n  FROM   t"), "SELECT * FROM t",);
    }
}
