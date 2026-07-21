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
#[cfg(test)]
pub mod tests {
    use std::process::Command;

    const MANIFEST_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../test-toasty/Cargo.toml");

    fn run_basic(format: Option<&str>) -> String {
        let mut cmd = Command::new("cargo");
        cmd.args([
            "run",
            "--manifest-path",
            MANIFEST_PATH,
            "--example",
            "basic",
            "--features",
            "hotpath",
        ]);
        if let Some(fmt) = format {
            cmd.env("HOTPATH_OUTPUT_FORMAT", fmt);
        }
        let output = cmd.output().expect("Failed to execute command");
        assert!(
            output.status.success(),
            "Command failed with status: {}",
            output.status
        );
        String::from_utf8_lossy(&output.stdout).into_owned()
    }

    #[test]
    fn test_table_output() {
        let stdout = run_basic(None);

        let all_expected = [
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
        ];
        for expected in all_expected {
            assert!(
                stdout.contains(expected),
                "Expected:\n{expected}\n\nGot:\n{stdout}",
            );
        }
    }

    #[test]
    fn test_transaction_queries_captured() {
        // 50 loop creates + 1 transaction-internal create = 51. The event is
        // emitted at the driver level, so transaction-internal queries are
        // captured too.
        let stdout = run_basic(Some("json"));

        let all_expected = [
            "\"sql\"",
            r#"INSERT INTO \"users\" (\"id\", \"name\", \"age\") VALUES (?, ?, ?);"#,
            "\"count\":51",
        ];
        for expected in all_expected {
            assert!(
                stdout.contains(expected),
                "Expected:\n{expected}\n\nGot:\n{stdout}",
            );
        }
    }
}
