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
#[cfg(test)]
pub mod tests {
    use std::process::Command;

    const MANIFEST_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../test-toasty/Cargo.toml");

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
                "--manifest-path",
                MANIFEST_PATH,
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
            "toasty postgres tracing-layer example completed!",
            "sql - SQL query execution time statistics.",
            // $1/$2/$3 placeholders normalize to ?.
            "INSERT INTO \"users\" (\"id\", \"name\", \"age\") VALUES (?, ?, ?);",
            "SELECT name FROM users WHERE age = ?",
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
}
