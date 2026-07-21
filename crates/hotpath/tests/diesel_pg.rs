//! Integration test for the Diesel `connection::Instrumentation` front-end
//! against a real PostgreSQL server.
//!
//! Runs the `test-diesel` `basic_postgres` example as a subprocess and asserts
//! on its report. Requires the PostgreSQL container from the repo-root
//! compose.yml (`docker compose up -d postgres`, host port 5439). Skips locally
//! when nothing listens there; on CI the postgres service is mandatory, so a
//! missing server fails instead.
//!
//! The example uses `$1` positional placeholders, so this pins their `?`
//! normalization for the Diesel path, plus the ` -- binds: [..]` stripping on
//! Pg's `DebugQuery` Display output. The example writes to `diesel_users`
//! (not `users`) because the database persists across runs and is shared with
//! the sqlx postgres test binaries, which drop/recreate `users` - so the
//! asserted bucket texts differ from the sqlite ones by table name only.
#[cfg(test)]
pub mod tests {
    use std::process::Command;

    fn postgres_available() -> bool {
        let addr = "127.0.0.1:5439".parse().unwrap();
        let timeout = std::time::Duration::from_millis(500);
        std::net::TcpStream::connect_timeout(&addr, timeout).is_ok()
    }

    fn skip_without_postgres(test: &str) -> bool {
        if postgres_available() {
            return false;
        }
        assert!(
            std::env::var_os("CI").is_none(),
            "no PostgreSQL on localhost:5439 - the CI postgres service is required"
        );
        eprintln!(
            "skipping {test}: no PostgreSQL on localhost:5439 \
             (start it with `docker compose up -d postgres`)"
        );
        true
    }

    fn run_basic(format: Option<&str>) -> String {
        let mut cmd = Command::new("cargo");
        cmd.args([
            "run",
            "-p",
            "test-diesel",
            "--example",
            "basic_postgres",
            "--features",
            "hotpath,pg",
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
    fn test_table_output_postgres() {
        if skip_without_postgres("test_table_output_postgres") {
            return;
        }

        let stdout = run_basic(None);

        let all_expected = [
            "Diesel postgres instrumentation example completed!",
            "sql - SQL query execution time statistics.",
            // $1/$2 placeholders normalize to ?, so all 50 inserts share one bucket.
            "INSERT INTO diesel_users (name, age) VALUES (?, ?)",
            "SELECT id, name, age FROM diesel_users WHERE id = ?",
            // Inline literals normalized into one bucket.
            "SELECT name FROM diesel_users WHERE age = ?",
            "SELECT COUNT(*) FROM diesel_users",
            // Different-arity IN lists collapse to one bucket.
            "SELECT * FROM diesel_users WHERE id IN (?)",
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
    fn test_transaction_query_captured_postgres() {
        if skip_without_postgres("test_transaction_query_captured_postgres") {
            return;
        }

        // 50 loop inserts + 1 transaction-internal insert = 51. The
        // transaction-internal query is captured; BEGIN/COMMIT are not.
        let stdout = run_basic(Some("json"));

        let all_expected = [
            "\"sql\"",
            "\"INSERT INTO diesel_users (name, age) VALUES (?, ?)\"",
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
