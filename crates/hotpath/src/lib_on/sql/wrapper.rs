//! Instrumented wrapper around a sqlx SQLite [`SqlitePool`].
//!
//! `&InstrumentedSqlitePool` implements sqlx's [`Executor`], so every query run
//! through it (`fetch_*`, `execute`, ...) is timed automatically. The trait
//! routes all execution through [`Executor::fetch_many`] and
//! [`Executor::fetch_optional`], so instrumenting just those two captures every
//! query path.
//!
//! As of sqlx 0.9 [`Execute::sql`] consumes the query, so we cannot peek the
//! statement text and still execute the original. Instead each instrumented
//! method decomposes the query into its parts (arguments + SQL text) and
//! rebuilds an equivalent executable (`query_with` for prepared queries,
//! `raw_sql` for the simple/unprepared protocol).

use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use futures_util::future::BoxFuture;
use futures_util::stream::BoxStream;
use futures_util::Stream;
use sqlx::pool::PoolConnection;
use sqlx::sqlite::{
    Sqlite, SqliteArguments, SqlitePool, SqliteQueryResult, SqliteRow, SqliteStatement,
    SqliteTypeInfo,
};
use sqlx::{Acquire, Describe, Either, Error, Execute, Executor, SqlStr, Transaction};

use crate::instant::Instant;
use crate::lib_on::sql::{init_sql_state, send_sql_event, SqlEvent};

/// Instrumented drop-in replacement for [`SqlitePool`].
///
/// Not constructed directly - use the [`sql!`](crate::sql) macro. Pass `&pool`
/// to sqlx query methods exactly as you would a real pool.
#[derive(Debug, Clone)]
pub struct InstrumentedSqlitePool {
    pool: SqlitePool,
}

/// Trait for instrumenting a sqlx pool. Not intended for direct use - use the
/// `sql!` macro instead.
#[doc(hidden)]
pub trait InstrumentSqlx {
    fn instrument(self) -> InstrumentedSqlitePool;
}

impl InstrumentSqlx for SqlitePool {
    fn instrument(self) -> InstrumentedSqlitePool {
        init_sql_state();
        InstrumentedSqlitePool { pool: self }
    }
}

impl InstrumentedSqlitePool {
    /// Returns a reference to the underlying pool.
    pub fn inner(&self) -> &SqlitePool {
        &self.pool
    }

    /// Inherent mirrors of the common [`sqlx::Pool`] methods so the wrapper is a
    /// drop-in at call sites that use method syntax (`pool.begin()`, ...) without
    /// importing [`sqlx::Acquire`]. Connections/transactions obtained this way are
    /// not instrumented - only queries run directly against the pool are.
    pub async fn begin(&self) -> Result<Transaction<'static, Sqlite>, Error> {
        self.pool.begin().await
    }

    pub async fn acquire(&self) -> Result<PoolConnection<Sqlite>, Error> {
        self.pool.acquire().await
    }

    pub async fn close(&self) {
        self.pool.close().await
    }
}

fn emit(sql: Arc<str>, start: Instant, is_error: bool) {
    let now = Instant::now();
    send_sql_event(SqlEvent::Executed {
        sql,
        duration_nanos: now.duration_since(start).as_nanos() as u64,
        is_error,
        elapsed_ns: crate::lib_on::elapsed_since_start_ns(now),
    });
}

/// Splits a query into its statement text, persistence flag, arguments, and the
/// owned [`SqlStr`] used to rebuild an executable. Consuming `sql()` is the only
/// way to read the text in sqlx 0.9.
fn decompose<'q, E>(mut query: E) -> (Arc<str>, bool, Option<SqliteArguments>, SqlStr)
where
    E: Execute<'q, Sqlite>,
{
    let persistent = query.persistent();
    let arguments = query.take_arguments().unwrap_or(None);
    let sql = query.sql();
    let text: Arc<str> = Arc::from(sql.as_str());
    (text, persistent, arguments, sql)
}

/// Wraps the inner result stream and emits a timing event when the stream is
/// dropped (fully consumed or abandoned early), measuring the whole call.
struct TimedStream<'e> {
    inner: BoxStream<'e, Result<Either<SqliteQueryResult, SqliteRow>, Error>>,
    sql: Arc<str>,
    start: Instant,
    sent: bool,
    errored: bool,
}

impl Stream for TimedStream<'_> {
    type Item = Result<Either<SqliteQueryResult, SqliteRow>, Error>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        let polled = this.inner.as_mut().poll_next(cx);
        if let Poll::Ready(Some(Err(_))) = &polled {
            this.errored = true;
        }
        polled
    }
}

impl Drop for TimedStream<'_> {
    fn drop(&mut self) {
        if !self.sent {
            self.sent = true;
            emit(self.sql.clone(), self.start, self.errored);
        }
    }
}

impl<'p> Executor<'p> for &'p InstrumentedSqlitePool {
    type Database = Sqlite;

    fn fetch_many<'e, 'q: 'e, E>(
        self,
        query: E,
    ) -> BoxStream<'e, Result<Either<SqliteQueryResult, SqliteRow>, Error>>
    where
        'p: 'e,
        E: 'q + Execute<'q, Sqlite>,
    {
        let (text, persistent, arguments, sql) = decompose(query);
        let start = Instant::now();
        let inner = match arguments {
            Some(args) => Executor::fetch_many(
                &self.pool,
                sqlx::query_with(sql, args).persistent(persistent),
            ),
            None => Executor::fetch_many(&self.pool, sqlx::raw_sql(sql)),
        };
        Box::pin(TimedStream {
            inner,
            sql: text,
            start,
            sent: false,
            errored: false,
        })
    }

    fn fetch_optional<'e, 'q: 'e, E>(
        self,
        query: E,
    ) -> BoxFuture<'e, Result<Option<SqliteRow>, Error>>
    where
        'p: 'e,
        E: 'q + Execute<'q, Sqlite>,
    {
        let (text, persistent, arguments, sql) = decompose(query);
        let start = Instant::now();
        let inner = match arguments {
            Some(args) => Executor::fetch_optional(
                &self.pool,
                sqlx::query_with(sql, args).persistent(persistent),
            ),
            None => Executor::fetch_optional(&self.pool, sqlx::raw_sql(sql)),
        };
        Box::pin(async move {
            let res = inner.await;
            let is_error = res.is_err();
            emit(text, start, is_error);
            res
        })
    }

    fn prepare_with<'e>(
        self,
        sql: SqlStr,
        parameters: &'e [SqliteTypeInfo],
    ) -> BoxFuture<'e, Result<SqliteStatement, Error>>
    where
        'p: 'e,
    {
        Executor::prepare_with(&self.pool, sql, parameters)
    }

    fn describe<'e>(self, sql: SqlStr) -> BoxFuture<'e, Result<Describe<Sqlite>, Error>>
    where
        'p: 'e,
    {
        Executor::describe(&self.pool, sql)
    }
}

/// Delegates to the inner pool so the wrapper is a drop-in for APIs that take an
/// [`Acquire`] (migrations, `begin()` transactions, ...). Queries run through an
/// acquired connection or transaction are not instrumented - only those issued
/// directly against `&InstrumentedSqlitePool` flow through the [`Executor`] impl.
impl<'a> Acquire<'a> for &'a InstrumentedSqlitePool {
    type Database = Sqlite;
    type Connection = PoolConnection<Sqlite>;

    fn acquire(self) -> BoxFuture<'a, Result<Self::Connection, Error>> {
        Acquire::acquire(&self.pool)
    }

    fn begin(self) -> BoxFuture<'a, Result<Transaction<'a, Sqlite>, Error>> {
        Acquire::begin(&self.pool)
    }
}
