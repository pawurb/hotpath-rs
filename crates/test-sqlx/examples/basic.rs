//! Demonstrates `hotpath::sql!` wrapping a sqlx SQLite pool so every query is
//! timed and aggregated by *normalized* statement text.
//!
//! Run with:
//!   cargo run -p test-sqlx --example basic --features hotpath
//!
//! Watch how parameter-varied queries collapse into single buckets:
//! - bind-parameter queries (`... WHERE id = ?`) already share one text
//! - inline-literal queries (`... WHERE age = 21`, `= 22`, ...) merge via normalization
//! - `IN (1,2,3)` and `IN (4,5,6,7,8)` both become `IN (?)`

use hotpath::{HotpathGuardBuilder, Section};
use sqlx::sqlite::SqlitePoolOptions;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _guard = HotpathGuardBuilder::new("main")
        .percentiles(&[50.0, 95.0, 99.0])
        .sections(vec![Section::Sql])
        .build();

    let raw_pool = SqlitePoolOptions::new()
        .max_connections(4)
        .connect("sqlite::memory:")
        .await?;

    // Wrap the pool: every query through `&pool` from here on is measured.
    let pool = hotpath::sql!(raw_pool);

    sqlx::query("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT, age INTEGER)")
        .execute(&pool)
        .await?;

    // 50 inserts, identical prepared text -> one bucket.
    for i in 0..50 {
        sqlx::query("INSERT INTO users (name, age) VALUES (?, ?)")
            .bind(format!("user{i}"))
            .bind(20 + i)
            .execute(&pool)
            .await?;
    }

    // 30 point lookups, bind params -> one bucket.
    for i in 1..=30 {
        let _ = sqlx::query("SELECT id, name, age FROM users WHERE id = ?")
            .bind(i)
            .fetch_optional(&pool)
            .await?;
    }

    // 20 selects with VARYING inline literals -> normalization merges them.
    // (sqlx 0.9 requires AssertSqlSafe for non-'static SQL strings.)
    for i in 1..=20 {
        let q = format!("SELECT name FROM users WHERE age = {}", 20 + i);
        let _ = sqlx::query(sqlx::AssertSqlSafe(q)).fetch_all(&pool).await?;
    }

    // IN-lists of different arity -> both collapse to `IN (?)`.
    let _ = sqlx::query("SELECT * FROM users WHERE id IN (1, 2, 3)")
        .fetch_all(&pool)
        .await?;
    let _ = sqlx::query("SELECT * FROM users WHERE id IN (4, 5, 6, 7, 8)")
        .fetch_all(&pool)
        .await?;

    // 10 aggregates -> one bucket.
    for _ in 0..10 {
        let _: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users")
            .fetch_one(&pool)
            .await?;
    }

    Ok(())
}
