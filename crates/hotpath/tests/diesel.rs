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
#[path = "common/support.rs"]
mod common;

#[cfg(test)]
pub mod tests {
    use crate::common::assert_contains_all;
    use crate::common::example::Example;

    fn basic() -> Example {
        Example::new("test-diesel", "basic")
    }

    #[test]
    fn test_table_output() {
        let stdout = basic().stdout();

        assert_contains_all(
            &stdout,
            &[
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
    fn test_transaction_query_captured() {
        // 50 loop inserts + 1 transaction-internal insert = 51. The
        // transaction-internal query is captured; BEGIN/COMMIT are not.
        let stdout = basic().json().stdout();

        assert_contains_all(
            &stdout,
            &[
                "\"sql\"",
                "\"INSERT INTO users (name, age) VALUES (?, ?)\"",
                "\"count\":51",
            ],
        );
    }
}
