//! Integration tests for the Toasty tracing-layer front-end (sqlite backend).
//!
//! These run the `test-toasty` `basic` example as a subprocess and assert on
//! its report. `test-toasty` is a standalone crate (not a workspace member -
//! toasty's rusqlite and the workspace's sqlx-sqlite have conflicting
//! `links = "sqlite3"` values), so it is built via `--manifest-path`.
//!
//! These also pin Toasty's `toasty::query` event field schema
//! (`db.statement` / `duration_ms`) - a Toasty upgrade that renames or drops
//! those fields would empty the SQL report and fail here.
//!
//! PostgreSQL coverage lives in `toasty_pg.rs`.
#[path = "common/support.rs"]
mod common;

#[cfg(test)]
pub mod tests {
    use crate::common::assert_contains_all;
    use crate::common::example::Example;

    const MANIFEST_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../test-toasty/Cargo.toml");

    fn basic() -> Example {
        Example::in_manifest(MANIFEST_PATH, "basic")
    }

    #[test]
    fn test_table_output() {
        let stdout = basic().stdout();

        assert_contains_all(
            &stdout,
            &[
                "toasty tracing-layer example completed!",
                "sql - SQL query execution time statistics.",
                // Model-generated INSERT; sqlite's `?1`/`?2` numbered placeholders
                // normalize to plain `?`.
                "INSERT INTO \"users\" (\"id\", \"name\", \"age\") VALUES (?, ?, ?);",
                // Model-generated point lookup (long statement, truncated in the
                // table cell).
                "SELECT tbl_0_0.",
                // Raw queries with inline literals normalized into one bucket.
                "SELECT name FROM users WHERE age = ?",
                // Different-arity IN lists collapse to one bucket.
                "SELECT name FROM users WHERE age IN (?)",
                "SELECT COUNT(*) FROM users",
            ],
        );
    }

    #[test]
    fn test_transaction_queries_captured() {
        // 50 loop creates + 1 transaction-internal create = 51. The event is
        // emitted at the driver level, so transaction-internal queries are
        // captured too.
        let stdout = basic().json().stdout();

        assert_contains_all(
            &stdout,
            &[
                "\"sql\"",
                r#"INSERT INTO \"users\" (\"id\", \"name\", \"age\") VALUES (?, ?, ?);"#,
                "\"count\":51",
            ],
        );
    }
}
