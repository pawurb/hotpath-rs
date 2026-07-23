//! `tracing_subscriber::Layer` front-end for Toasty ORM SQL query profiling.
//!
//! Every Toasty driver funnels each physical database operation through
//! toasty-core's `QueryLog`, which emits one completed-query event per
//! execution through the `tracing` facade (at `DEBUG`, escalated to `WARN`
//! past the configured slow-statement threshold), with fields following the
//! OpenTelemetry database semantic conventions:
//!
//! ```text
//! target = "toasty::query"
//! fields: duration_ms = <f64>, rows, error, db.system,
//!         db.statement = "<SQL>", db.operation, db.collection, db.params
//! ```
//!
//! [`HotpathToastyLayer`] observes these and forwards each one to
//! [`send_sql_event`]. The event is emitted *after* completion with Toasty's
//! own measured `duration_ms`, so we never time anything ourselves and there
//! is no start/finish pairing. Like the sqlx layer, this has no dependency on
//! the instrumented library - it only reads `tracing` event fields - so it
//! covers every Toasty SQL driver (SQLite, PostgreSQL, MySQL, Turso) and
//! captures transaction-internal queries too.
//!
//! Key-value drivers (DynamoDB) emit `db.operation`/`db.collection` instead
//! of `db.statement`; those events carry no SQL and are skipped. Failed
//! queries are recorded - the time was spent - and the `error` field is
//! ignored.

use tracing::field::{Field, Visit};
use tracing::subscriber::Interest;
use tracing::{Event, Metadata, Subscriber};
use tracing_subscriber::layer::{Context, Filter};
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::Layer;

use crate::lib_on::sql::{init_sql_state, send_sql_event, SqlEvent};

/// Target toasty-core's `QueryLog` emits its per-query completion event on.
const TOASTY_QUERY_TARGET: &str = "toasty::query";

/// Layer that turns Toasty `toasty::query` events into [`SqlEvent`]s.
pub(crate) struct HotpathToastyLayer;

impl<S> Layer<S> for HotpathToastyLayer
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let mut visitor = QueryVisitor::default();
        event.record(&mut visitor);

        // Absent for key-value (DynamoDB) operations - nothing to report.
        let Some(sql) = visitor.statement else {
            return;
        };

        send_sql_event(SqlEvent::Executed {
            sql: sql.into(),
            duration_nanos: visitor.duration_ns.unwrap_or(0),
            timestamp_ns: crate::lib_on::current_elapsed_ns(),
        });
    }
}

/// Extracts the statement text and execution time from a `toasty::query`
/// event.
///
/// Toasty records `db.statement` wrapped in `tracing::field::display(..)`, so
/// it arrives through `record_debug` (whose `{:?}` formatting forwards to the
/// inner `Display`, yielding the bare SQL) rather than `record_str`. Both are
/// handled in case the recording strategy changes.
#[derive(Default)]
struct QueryVisitor {
    statement: Option<String>,
    duration_ns: Option<u64>,
}

impl Visit for QueryVisitor {
    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "db.statement" {
            self.statement = Some(value.trim().to_string());
        }
    }

    fn record_f64(&mut self, field: &Field, value: f64) {
        if field.name() == "duration_ms" {
            self.duration_ns = Some((value * 1e6) as u64);
        }
    }

    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if field.name() == "db.statement" && self.statement.is_none() {
            self.statement = Some(format!("{value:?}").trim().to_string());
        }
    }
}

/// Per-layer filter that admits only Toasty's `toasty::query` events to
/// [`HotpathToastyLayer`], leaving every other layer in the subscriber
/// untouched.
///
/// Deliberately reports no `max_level_hint`: the events fire at `DEBUG`, and
/// Toasty gates its parameter rendering on
/// `tracing::event_enabled!(target: "toasty::query", DEBUG)`, so a hint below
/// `DEBUG` would suppress the events entirely. Returning `None` here keeps
/// the callsites enabled; `callsite_enabled` still permanently disables
/// non-Toasty callsites for this layer, so there is no per-event cost for
/// unrelated targets.
struct ToastyQueryFilter;

impl<S> Filter<S> for ToastyQueryFilter {
    fn enabled(&self, meta: &Metadata<'_>, _ctx: &Context<'_, S>) -> bool {
        meta.target() == TOASTY_QUERY_TARGET
    }

    fn callsite_enabled(&self, meta: &Metadata<'_>) -> Interest {
        if meta.target() == TOASTY_QUERY_TARGET {
            Interest::always()
        } else {
            Interest::never()
        }
    }
}

/// Builds the hotpath SQL profiling layer for the Toasty ORM. Spawns the
/// `hp-sql` worker and the metrics server on first call, and bakes in a
/// per-layer filter so only Toasty's `toasty::query` events reach it - the
/// user configures nothing about filtering.
///
/// Add it once when building your `tracing` subscriber:
///
/// ```rust,no_run
/// use tracing_subscriber::prelude::*;
///
/// tracing_subscriber::registry()
///     .with(hotpath::toasty_tracing_layer())
///     .init();
/// ```
///
/// Requires the `toasty` feature.
///
/// Caveat: a *global* `EnvFilter` (`registry().with(env_filter)`) runs before
/// this per-layer filter and can suppress `toasty::query` for the whole
/// stack. Attach any `EnvFilter` per-layer instead, or don't globally filter
/// out the `toasty::query` target.
pub fn toasty_tracing_layer<S>() -> impl Layer<S>
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    init_sql_state();
    HotpathToastyLayer.with_filter(ToastyQueryFilter)
}
