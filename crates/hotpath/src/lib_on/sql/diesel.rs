//! Diesel front-end for SQL query profiling.
//!
//! Diesel emits nothing through `tracing`, so the `sqlx` layer approach does not
//! transfer. It instead exposes a first-class callback trait,
//! [`diesel::connection::Instrumentation`] (stable since Diesel 2.2), which fires
//! per connection event. [`HotpathDieselInstrumentation`] watches the
//! `StartQuery`/`FinishQuery` pair, times it with [`Instant`], and forwards each
//! completed query to [`send_sql_event`] - the same pipeline the `sqlx` layer
//! feeds, so everything downstream (worker, normalization, report, TUI) is shared.
//!
//! Diesel gives no `elapsed` of its own, so we measure the wall-clock span from
//! `StartQuery` to `FinishQuery` ourselves (includes row streaming). A connection
//! executes queries serially, so a single `pending` slot is sufficient.

use diesel::connection::{set_default_instrumentation, Instrumentation, InstrumentationEvent};

use crate::instant::Instant;
use crate::lib_on::sql::{init_sql_state, send_sql_event, SqlEvent};

#[derive(Default)]
struct HotpathDieselInstrumentation {
    pending: Option<(String, Instant)>,
}

impl Instrumentation for HotpathDieselInstrumentation {
    fn on_connection_event(&mut self, event: InstrumentationEvent<'_>) {
        match event {
            InstrumentationEvent::StartQuery { query, .. } => {
                let sql = clean_sql(&query.to_string());
                // Transaction-control statements (BEGIN/COMMIT/...) also arrive as
                // queries via `batch_execute`; clearing `pending` drops the pair so
                // the report stays queries-only.
                self.pending = (!is_transaction_control(&sql)).then(|| (sql, Instant::now()));
            }
            InstrumentationEvent::FinishQuery { .. } => {
                if let Some((sql, start)) = self.pending.take() {
                    let now = Instant::now();
                    send_sql_event(SqlEvent::Executed {
                        sql: sql.into(),
                        duration_nanos: now.duration_since(start).as_nanos() as u64,
                        elapsed_ns: crate::lib_on::elapsed_since_start_ns(now),
                    });
                }
            }
            _ => {}
        }
    }
}

/// Strips the ` -- binds: [..]` suffix Diesel's `DebugQuery` Display appends, so
/// parameter-varied executions normalize to the same bucket.
///
/// Diesel always appends the bind list at the *end*, so split from the right -
/// stripping from the left would truncate a query that itself contains the
/// `-- binds:` marker (e.g. in a comment or string literal).
fn clean_sql(rendered: &str) -> String {
    match rendered.rsplit_once(" -- binds:") {
        Some((sql, _)) => sql.trim().to_string(),
        None => rendered.trim().to_string(),
    }
}

/// Whether `sql` is a transaction-control statement Diesel issues itself
/// (`BEGIN`, `COMMIT`, `ROLLBACK`, `SAVEPOINT ...`, `RELEASE SAVEPOINT ...`).
fn is_transaction_control(sql: &str) -> bool {
    // Compare on bytes: the keywords are ASCII, and byte slicing avoids the
    // char-boundary panic that `&str` indexing would hit on Unicode-leading SQL.
    let head = sql.trim_start().as_bytes();
    ["BEGIN", "COMMIT", "ROLLBACK", "SAVEPOINT", "RELEASE"]
        .iter()
        .any(|kw| {
            let kw = kw.as_bytes();
            head.len() >= kw.len()
                && head[..kw.len()].eq_ignore_ascii_case(kw)
                // Word boundary: keyword is the whole statement or followed by space.
                && head.get(kw.len()).is_none_or(|b| b.is_ascii_whitespace())
        })
}

/// Factory the Diesel default-instrumentation slot requires: a plain `fn`
/// pointer, not a capturing closure.
fn diesel_instrumentation() -> Option<Box<dyn Instrumentation>> {
    Some(Box::<HotpathDieselInstrumentation>::default())
}

/// Installs hotpath SQL profiling for Diesel. Spawns the `hp-sql` worker and the
/// metrics server on first call, then registers the instrumentation as the
/// default for every newly-established connection:
///
/// ```rust,no_run
/// hotpath::instrument_diesel_sql();
/// // open connections AFTER this call so they pick up the instrumentation
/// ```
///
/// Requires the `diesel` feature. Connections established before this call are
/// not instrumented.
pub fn instrument_diesel_sql() {
    init_sql_state();
    let _ = set_default_instrumentation(diesel_instrumentation);
}

#[cfg(test)]
mod tests {
    use crate::lib_on::sql::diesel::{clean_sql, is_transaction_control};

    #[test]
    fn strips_binds_suffix() {
        assert_eq!(
            clean_sql("INSERT INTO t (a) VALUES (?) -- binds: [\"x\"]"),
            "INSERT INTO t (a) VALUES (?)",
        );
        assert_eq!(
            clean_sql("SELECT COUNT(*) FROM t"),
            "SELECT COUNT(*) FROM t"
        );
        // Only Diesel's final appended suffix is stripped; a `-- binds:` marker
        // inside the query text is preserved (strip from the right, not the left).
        assert_eq!(
            clean_sql("SELECT 1 -- binds: note -- binds: [42]"),
            "SELECT 1 -- binds: note",
        );
    }

    #[test]
    fn detects_transaction_control() {
        for sql in [
            "BEGIN",
            "COMMIT",
            "ROLLBACK",
            "SAVEPOINT diesel_savepoint_1",
        ] {
            assert!(is_transaction_control(sql), "{sql} should be control");
        }
        for sql in [
            "SELECT 1",
            "INSERT INTO t (a) VALUES (?)",
            "BEGINNER",
            // Unicode-leading SQL must not panic on byte-boundary slicing.
            "-- 😀 SELECT 1",
            "😀",
        ] {
            assert!(!is_transaction_control(sql), "{sql} should be a query");
        }
    }
}
