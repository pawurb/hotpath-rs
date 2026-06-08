//! Integration tests for the `sqlx` tracing-layer front-end.
//!
//! These run the `test-sqlx` `basic` example as a subprocess and assert on its
//! report. They also pin sqlx's `sqlx::query` event field schema (`db.statement`
//! / `summary` / `elapsed_secs`): a sqlx upgrade that renames or drops those
//! fields would empty the SQL report and fail these tests.
#[cfg(test)]
pub mod tests {
    use std::process::Command;

    fn run_basic(format: Option<&str>) -> String {
        let mut cmd = Command::new("cargo");
        cmd.args([
            "run",
            "-p",
            "test-sqlx",
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
            "sqlx tracing-layer example completed!",
            "sql - SQL query execution time statistics.",
            "INSERT INTO users (name, age) VALUES (?, ?)",
            // Short query (4 words) arrives via `summary`, not `db.statement`.
            "SELECT COUNT(*) FROM users",
            // Inline literals normalized into one bucket.
            "SELECT name FROM users WHERE age = ?",
            // Different-arity IN lists collapse to one bucket.
            "SELECT * FROM users WHERE id IN (?)",
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
        // 50 loop inserts + 1 transaction-internal insert = 51. The old pool
        // wrapper missed transaction-internal queries; the layer captures them.
        let stdout = run_basic(Some("json"));

        let all_expected = [
            "\"sql\"",
            "\"INSERT INTO users (name, age) VALUES (?, ?)\"",
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
