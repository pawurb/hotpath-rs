//! Integration tests for the `sqlx` tracing-layer front-end (PostgreSQL backend).
//!
//! Runs the `test-sqlx-08` `basic_postgres` example as a subprocess and asserts
//! on its report. Requires the PostgreSQL container from the repo-root
//! compose.yml (`docker compose up -d postgres`, host port 5439). Skips locally
//! when nothing listens there; on CI the postgres service is mandatory, so a
//! missing server fails instead.
//!
//! The example uses `$1` positional placeholders, so this also pins their
//! normalization to `?` - the asserted bucket texts are identical to the
//! sqlite ones in `sql_sqlite.rs`.
#[cfg(test)]
pub mod tests {
    use std::process::Command;

    fn postgres_available() -> bool {
        let addr = "127.0.0.1:5439".parse().unwrap();
        let timeout = std::time::Duration::from_millis(500);
        std::net::TcpStream::connect_timeout(&addr, timeout).is_ok()
    }

    #[test]
    fn test_table_output_postgres() {
        if !postgres_available() {
            assert!(
                std::env::var_os("CI").is_none(),
                "no PostgreSQL on localhost:5439 - the CI postgres service is required"
            );
            eprintln!(
                "skipping test_table_output_postgres: no PostgreSQL on localhost:5439 \
                 (start it with `docker compose up -d postgres`)"
            );
            return;
        }

        let output = Command::new("cargo")
            .args([
                "run",
                "-p",
                "test-sqlx-08",
                "--example",
                "basic_postgres",
                "--features",
                "hotpath",
            ])
            .output()
            .expect("Failed to execute command");
        assert!(
            output.status.success(),
            "Command failed with status: {}",
            output.status
        );
        let stdout = String::from_utf8_lossy(&output.stdout);

        let all_expected = [
            "sqlx 0.8 postgres tracing-layer example completed!",
            "sql - SQL query execution time statistics.",
            // $1/$2 placeholders normalize to ?.
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
}
