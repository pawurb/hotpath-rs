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
#[path = "common/support.rs"]
mod common;

#[cfg(all(test, feature = "hotpath"))]
pub mod tests {
    use crate::common::assert_contains_all;
    use crate::common::example::Example;
    use crate::common::service::POSTGRES;

    fn example(name: &str) -> Example {
        Example::new("test-sqlx-08", name)
    }

    #[test]
    fn test_table_output_postgres() {
        if POSTGRES.skip_if_unavailable("test_table_output_postgres") {
            return;
        }

        let stdout = example("basic_postgres").stdout();

        assert_contains_all(
            &stdout,
            &[
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
            ],
        );
    }

    // The same statement executed from two instrumented functions splits into
    // per-source entries; queries outside any measured scope have no source,
    // and a nested measured call attributes to the innermost function. Only
    // covered on PostgreSQL: its driver executes queries on the calling task,
    // while sqlite runs them on a connection worker thread (no source there).
    #[test]
    fn test_sql_sources_postgres() {
        if POSTGRES.skip_if_unavailable("test_sql_sources_postgres") {
            return;
        }

        let report = example("sources_postgres").json().report();
        let sql = report.sql.expect("No sql section in report");

        let insert_query = "INSERT INTO users (name, age) VALUES (?, ?)";
        let inserts: Vec<_> = sql
            .data
            .iter()
            .filter(|e| e.query == insert_query)
            .collect();
        assert_eq!(
            inserts.len(),
            3,
            "expected one entry per source: {inserts:?}"
        );

        // `insert_from_a` runs once directly and once inside `outer` - both
        // executions attribute to the innermost function.
        let from_a = inserts
            .iter()
            .find(|e| e.source.as_deref() == Some("sources_postgres::insert_from_a"))
            .expect("insert_from_a entry missing");
        assert_eq!(from_a.count, 2);

        let from_b = inserts
            .iter()
            .find(|e| e.source.as_deref() == Some("sources_postgres::insert_from_b"))
            .expect("insert_from_b entry missing");
        assert_eq!(from_b.count, 2);

        let unattributed = inserts
            .iter()
            .find(|e| e.source.is_none())
            .expect("source-less entry missing");
        assert_eq!(unattributed.count, 3);

        // DDL runs directly in main (wrapper scope): no source.
        let ddl = sql
            .data
            .iter()
            .find(|e| e.query.starts_with("CREATE TABLE users"))
            .expect("CREATE TABLE entry missing");
        assert!(ddl.source.is_none(), "wrapper guard must not attribute");
    }
}
