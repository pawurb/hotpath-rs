//! Integration tests for the Toasty tracing-layer front-end (PostgreSQL
//! backend).
//!
//! Runs the `test-toasty` `basic_postgres` example as a subprocess and asserts
//! on its report. Requires the PostgreSQL container from the repo-root
//! compose.yml (`docker compose up -d postgres`, host port 5439). Skips
//! locally when nothing listens there; on CI the postgres service is
//! mandatory, so a missing server fails instead.
//!
//! Toasty's PostgreSQL driver uses `$1` positional placeholders, so this also
//! pins their normalization to `?` - the asserted bucket texts are identical
//! to the sqlite ones in `toasty_sqlite.rs`.
#[path = "common/support.rs"]
mod common;

#[cfg(test)]
pub mod tests {
    use crate::common::assert_contains_all;
    use crate::common::example::Example;
    use crate::common::service::POSTGRES;

    const MANIFEST_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../test-toasty/Cargo.toml");

    #[test]
    fn test_table_output_postgres() {
        if POSTGRES.skip_if_unavailable("test_table_output_postgres") {
            return;
        }

        let stdout = Example::in_manifest(MANIFEST_PATH, "basic_postgres").stdout();

        assert_contains_all(
            &stdout,
            &[
                "toasty postgres tracing-layer example completed!",
                "sql - SQL query execution time statistics.",
                // $1/$2/$3 placeholders normalize to ?.
                "INSERT INTO \"users\" (\"id\", \"name\", \"age\") VALUES (?, ?, ?);",
                "SELECT name FROM users WHERE age = ?",
                "SELECT name FROM users WHERE age IN (?)",
                "SELECT COUNT(*) FROM users",
            ],
        );
    }
}
