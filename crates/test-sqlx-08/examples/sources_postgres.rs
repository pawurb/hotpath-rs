//! Demonstrates SQL source attribution: the same statement executed from two
//! instrumented functions lands in two report entries, each tagged with the
//! function it ran from. Queries executed outside any measured scope get no
//! source.
//!
//! Uses PostgreSQL because its driver executes queries on the calling task, so
//! the `sqlx::query` event fires inside the instrumented function's poll. The
//! sqlite driver runs statements on a dedicated connection worker thread, so
//! its events carry no source.
//!
//! Start the database first:
//!   docker compose up -d postgres
//!
//! Run with:
//!   cargo run -p test-sqlx-08 --example sources_postgres --features hotpath

use hotpath::{HotpathGuardBuilder, Section};
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use tracing_subscriber::prelude::*;

#[hotpath::measure]
async fn insert_from_a(pool: &PgPool) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO users (name, age) VALUES ($1, $2)")
        .bind("a")
        .bind(1)
        .execute(pool)
        .await?;
    Ok(())
}

#[hotpath::measure]
async fn insert_from_b(pool: &PgPool) -> Result<(), sqlx::Error> {
    for i in 0..2 {
        sqlx::query("INSERT INTO users (name, age) VALUES ($1, $2)")
            .bind("b")
            .bind(i)
            .execute(pool)
            .await?;
    }
    Ok(())
}

// Attribution picks the innermost instrumented function: queries issued by
// `insert_from_a` stay attributed to it even when it runs inside another
// measured function.
#[hotpath::measure]
async fn outer(pool: &PgPool) -> Result<(), sqlx::Error> {
    insert_from_a(pool).await
}

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
        .max_connections(1)
        .connect(&url)
        .await?;

    sqlx::query("DROP TABLE IF EXISTS users")
        .execute(&pool)
        .await?;
    sqlx::query("CREATE TABLE users (id SERIAL PRIMARY KEY, name TEXT, age INTEGER)")
        .execute(&pool)
        .await?;

    insert_from_a(&pool).await?;
    insert_from_b(&pool).await?;
    outer(&pool).await?;

    // Same statement outside any measured scope: separate, source-less entry.
    for i in 0..3 {
        sqlx::query("INSERT INTO users (name, age) VALUES ($1, $2)")
            .bind("plain")
            .bind(i)
            .execute(&pool)
            .await?;
    }

    println!("sqlx sources example completed!");
    Ok(())
}
