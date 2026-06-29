//! Demonstrates `hotpath::sqlx_tracing_layer()` capturing every sqlx 0.8 query
//! via a `tracing` layer - no pool wrapping, no application type changes. This
//! is the same layer used for sqlx 0.9; the `sqlx::query` event field schema is
//! identical across both versions, so only the dynamic-SQL call site differs
//! (0.8 takes `&str` directly; 0.9 needs `AssertSqlSafe`).
//!
//! Run with:
//!   cargo run -p test-sqlx-08 --example basic --features hotpath

use hotpath::{HotpathGuardBuilder, Section};
use sqlx::sqlite::SqlitePoolOptions;
use tracing_subscriber::prelude::*;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::registry()
        .with(hotpath::sqlx_tracing_layer())
        .init();

    let _guard = HotpathGuardBuilder::new("main")
        .sections(vec![Section::Sql])
        .build();

    let pool = SqlitePoolOptions::new()
        .max_connections(4)
        .connect("sqlite::memory:")
        .await?;

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
    // (sqlx 0.8 accepts a borrowed `&str` directly, no AssertSqlSafe.)
    for i in 1..=20 {
        let q = format!("SELECT name FROM users WHERE age = {}", 20 + i);
        let _ = sqlx::query(&q).fetch_all(&pool).await?;
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

    // Transaction-internal queries are captured too (a pool wrapper would miss these).
    let mut tx = pool.begin().await?;
    sqlx::query("INSERT INTO users (name, age) VALUES (?, ?)")
        .bind("in_tx")
        .bind(99)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;

    println!("sqlx 0.8 tracing-layer example completed!");
    Ok(())
}
