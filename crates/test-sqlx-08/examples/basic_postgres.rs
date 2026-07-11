//! Same tracing-layer demo as `basic.rs`, but against a real PostgreSQL
//! server. Exercises the PostgreSQL positional placeholder syntax (`$1`, `$2`)
//! which normalization merges into `?` buckets, matching the sqlite output.
//!
//! Start the database first:
//!   docker compose up -d postgres
//!
//! Run with:
//!   cargo run -p test-sqlx-08 --example basic_postgres --features hotpath

use hotpath::{HotpathGuardBuilder, Section};
use sqlx::postgres::PgPoolOptions;
use tracing_subscriber::prelude::*;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::registry()
        .with(hotpath::sqlx_tracing_layer())
        .init();

    let _guard = HotpathGuardBuilder::new("main")
        .sections(vec![Section::Sql])
        .build();

    let url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://hotpath:hotpath@localhost:5439/hotpath".to_string());

    let pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(&url)
        .await?;

    // The database persists across runs, unlike sqlite::memory:.
    sqlx::query("DROP TABLE IF EXISTS users")
        .execute(&pool)
        .await?;
    sqlx::query("CREATE TABLE users (id SERIAL PRIMARY KEY, name TEXT, age INTEGER)")
        .execute(&pool)
        .await?;

    // 50 inserts, identical prepared text -> one bucket ($1/$2 normalize to ?).
    for i in 0..50 {
        sqlx::query("INSERT INTO users (name, age) VALUES ($1, $2)")
            .bind(format!("user{i}"))
            .bind(20 + i)
            .execute(&pool)
            .await?;
    }

    // 30 point lookups, bind params -> one bucket.
    for i in 1..=30 {
        let _ = sqlx::query("SELECT id, name, age FROM users WHERE id = $1")
            .bind(i)
            .fetch_optional(&pool)
            .await?;
    }

    // 20 selects with VARYING inline literals -> normalization merges them.
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
    sqlx::query("INSERT INTO users (name, age) VALUES ($1, $2)")
        .bind("in_tx")
        .bind(99)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;

    println!("sqlx 0.8 postgres tracing-layer example completed!");

    // Keeps the process (and metrics server) alive so integration tests can
    // poll the HTTP endpoints mid-run.
    if let Ok(secs) = std::env::var("TEST_SLEEP_SECONDS") {
        let secs: u64 = secs.parse().unwrap_or(0);
        tokio::time::sleep(std::time::Duration::from_secs(secs)).await;
    }

    Ok(())
}
