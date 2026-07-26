//! `tracing_subscriber::Layer` front-end for SQL query profiling.
//!
//! sqlx has no dedicated instrumentation trait, but `sqlx-core` emits one
//! completed-query event per execution through the `tracing` facade:
//!
//! ```text
//! target = "sqlx::query"
//! fields: summary, db.statement = "<SQL>", rows_affected, rows_returned,
//!         elapsed = <Duration>, elapsed_secs = <f64>
//! ```
//!
//! This field schema is identical across sqlx 0.8 and 0.9, and the layer has no
//! sqlx dependency of its own - it only reads `tracing` event fields - so a
//! single layer works for both versions.
//!
//! [`HotpathSqlLayer`] observes these and forwards each one to
//! [`send_sql_event`]. Because the event is emitted *after* completion with
//! sqlx's own measured `elapsed`, we never time anything ourselves and there is
//! no start/finish pairing.
//!
//! Unlike a pool wrapper, this captures transaction-internal and
//! acquired-connection queries too (logging is at the statement level) and
//! requires zero application type changes - the pool stays a `sqlx::SqlitePool`.

use tracing::field::{Field, Visit};
use tracing::subscriber::Interest;
use tracing::{Event, Metadata, Subscriber};
use tracing_subscriber::layer::{Context, Filter};
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::Layer;

use crate::lib_on::sql::{init_sql_state, send_sql_event, SqlEvent};

/// Target sqlx emits its per-query completion event on.
const SQLX_QUERY_TARGET: &str = "sqlx::query";

/// Layer that turns sqlx `sqlx::query` events into [`SqlEvent`]s.
pub(crate) struct HotpathSqlLayer;

impl<S> Layer<S> for HotpathSqlLayer
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let mut visitor = QueryVisitor::default();
        event.record(&mut visitor);

        // sqlx puts the full SQL in `db.statement` only when it differs from the
        // 4-word `summary` (longer queries); for short queries `db.statement` is
        // empty and `summary` holds the whole text. Prefer the full statement,
        // fall back to the summary.
        let Some(sql) = visitor.statement.or(visitor.summary) else {
            return;
        };

        send_sql_event(SqlEvent::Executed {
            sql: sql.into(),
            duration_nanos: visitor.elapsed_ns.unwrap_or(0),
            timestamp_ns: crate::lib_on::current_elapsed_ns(),
            source: crate::lib_on::caller_stack::current_caller(),
        });
    }
}

/// Extracts the statement text and execution time from a `sqlx::query` event.
///
/// Prefers the `elapsed_secs` f64 field (exact) and falls back to parsing the
/// `Debug`-formatted `elapsed` `Duration` if only that is present.
#[derive(Default)]
struct QueryVisitor {
    statement: Option<String>,
    summary: Option<String>,
    elapsed_ns: Option<u64>,
}

impl Visit for QueryVisitor {
    fn record_str(&mut self, field: &Field, value: &str) {
        match field.name() {
            // Newline-wrapped full SQL; empty for queries that fit their summary.
            "db.statement" => {
                let trimmed = value.trim();
                if !trimmed.is_empty() {
                    self.statement = Some(trimmed.to_string());
                }
            }
            "summary" => self.summary = Some(value.trim().to_string()),
            _ => {}
        }
    }

    fn record_f64(&mut self, field: &Field, value: f64) {
        if field.name() == "elapsed_secs" {
            self.elapsed_ns = Some((value * 1e9) as u64);
        }
    }

    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        // Only used if `elapsed_secs` was absent.
        if field.name() == "elapsed" && self.elapsed_ns.is_none() {
            self.elapsed_ns = parse_duration_debug(&format!("{value:?}"));
        }
    }
}

/// Parses a `Duration`'s `Debug` form (e.g. `"1.234ms"`, `"56µs"`, `"2s"`,
/// `"700ns"`) into nanoseconds. Used only as a fallback when sqlx's
/// `elapsed_secs` f64 field is unavailable.
fn parse_duration_debug(s: &str) -> Option<u64> {
    let s = s.trim();
    let (num, scale) = if let Some(rest) = s.strip_suffix("ns") {
        (rest, 1.0)
    } else if let Some(rest) = s.strip_suffix("µs").or_else(|| s.strip_suffix("us")) {
        (rest, 1_000.0)
    } else if let Some(rest) = s.strip_suffix("ms") {
        (rest, 1_000_000.0)
    } else {
        let rest = s.strip_suffix('s')?;
        (rest, 1_000_000_000.0)
    };
    num.trim().parse::<f64>().ok().map(|v| (v * scale) as u64)
}

/// Per-layer filter that admits only sqlx's `sqlx::query` events to
/// [`HotpathSqlLayer`], leaving every other layer in the subscriber untouched.
///
/// Deliberately reports no `max_level_hint`: a `Some(INFO)` hint (as
/// `tracing_subscriber::filter::Targets` returns) makes sqlx's
/// `tracing::enabled!(target: "sqlx::query", INFO)` callsite check fail, so its
/// events are never emitted. Returning `None` here keeps the callsite enabled;
/// `callsite_enabled` still permanently disables non-sqlx callsites for this
/// layer, so there is no per-event cost for unrelated targets.
struct SqlxQueryFilter;

impl<S> Filter<S> for SqlxQueryFilter {
    fn enabled(&self, meta: &Metadata<'_>, _ctx: &Context<'_, S>) -> bool {
        meta.target() == SQLX_QUERY_TARGET
    }

    fn callsite_enabled(&self, meta: &Metadata<'_>) -> Interest {
        if meta.target() == SQLX_QUERY_TARGET {
            Interest::always()
        } else {
            Interest::never()
        }
    }
}

/// Builds the hotpath SQL profiling layer. Spawns the `hp-sql` worker and the
/// metrics server on first call, and bakes in a per-layer filter so only
/// sqlx's `sqlx::query` events reach it - the user configures nothing about
/// filtering.
///
/// Add it once when building your `tracing` subscriber:
///
/// ```rust,no_run
/// use tracing_subscriber::prelude::*;
///
/// tracing_subscriber::registry()
///     .with(hotpath::sqlx_tracing_layer())
///     .init();
/// ```
///
/// Requires the `sqlx` feature. Works with sqlx 0.8 and 0.9.
///
/// Caveat: a *global* `EnvFilter` (`registry().with(env_filter)`) runs before
/// this per-layer filter and can suppress `sqlx::query` for the whole stack.
/// Attach any `EnvFilter` per-layer instead, or don't globally filter out the
/// `sqlx::query` target.
pub fn sqlx_tracing_layer<S>() -> impl Layer<S>
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    init_sql_state();
    HotpathSqlLayer.with_filter(SqlxQueryFilter)
}
