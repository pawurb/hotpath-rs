//! Integration test for the Diesel `connection::Instrumentation` front-end.
//!
//! Runs the `test-diesel` `basic` example as a subprocess and asserts on its
//! report. Diesel emits nothing through `tracing`, so unlike the sqlx front-end
//! this drives `diesel::connection::Instrumentation` directly; the captured
//! queries feed the same downstream pipeline (normalization, report, JSON), so
//! parameter-varied executions collapse into one bucket exactly as for sqlx.
//!
//! This also pins the `InstrumentationEvent` shape and Diesel's `DebugQuery`
//! Display format (` -- binds: [..]`, stripped before normalization) - a Diesel
//! upgrade that changes either would surface here.
#[cfg(test)]
pub mod tests {
    use std::process::Command;

    fn run_basic(format: Option<&str>) -> String {
        let mut cmd = Command::new("cargo");
        cmd.args([
            "run",
            "-p",
            "test-diesel",
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
            "Diesel instrumentation example completed!",
            "sql - SQL query execution time statistics.",
            "INSERT INTO users (name, age) VALUES (?, ?)",
            // Bind values are stripped, so all 50 inserts share one bucket.
            "SELECT id, name, age FROM users WHERE id = ?",
            // Inline literals normalized into one bucket.
            "SELECT name FROM users WHERE age = ?",
            "SELECT COUNT(*) FROM users",
            // Different-arity IN lists collapse to one bucket.
            "SELECT * FROM users WHERE id IN (?)",
        ];
        for expected in all_expected {
            assert!(
                stdout.contains(expected),
                "Expected:\n{expected}\n\nGot:\n{stdout}",
            );
        }

        // Transaction-control statements must not surface as query buckets.
        for control in ["| BEGIN", "| COMMIT", "| ROLLBACK"] {
            assert!(
                !stdout.contains(control),
                "Unexpected transaction-control bucket {control:?} in:\n{stdout}",
            );
        }
    }

    #[test]
    fn test_transaction_query_captured() {
        // 50 loop inserts + 1 transaction-internal insert = 51. The
        // transaction-internal query is captured; BEGIN/COMMIT are not.
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
