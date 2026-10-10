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
#[path = "common/support.rs"]
mod common;

#[cfg(test)]
pub mod tests {
    use crate::common::assert_contains_all;
    use crate::common::example::Example;
    use crate::common::service::POSTGRES;

    fn basic() -> Example {
        Example::new("test-diesel", "basic_postgres").features("hotpath,pg")
    }

    #[test]
    fn test_table_output_postgres() {
        if POSTGRES.skip_if_unavailable("test_table_output_postgres") {
            return;
        }

        let stdout = basic().stdout();

        assert_contains_all(
            &stdout,
            &[
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
            ],
        );

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
        if POSTGRES.skip_if_unavailable("test_transaction_query_captured_postgres") {
            return;
        }

        // 50 loop inserts + 1 transaction-internal insert = 51. The
        // transaction-internal query is captured; BEGIN/COMMIT are not.
        let stdout = basic().json().stdout();

        assert_contains_all(
            &stdout,
            &[
                "\"sql\"",
                "\"INSERT INTO diesel_users (name, age) VALUES (?, ?)\"",
                "\"count\":51",
            ],
        );
    }
}
