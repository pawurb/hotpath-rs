//! Long-running variant (sqlx 0.8) that keeps issuing queries so the metrics
//! server stays up for live TUI / endpoint inspection.
//!
//! Terminal 1:
//!   cargo run -p test-sqlx-08 --example long_running --features hotpath
//! Terminal 2:
//!   cargo run --bin hotpath --features tui -- console --metrics-port 6770
//!   # then press [3] for the I/O tab and view the SQL sub-tab
//!
//! Or just inspect the raw endpoint: curl -s localhost:6770/sql

use hotpath::{HotpathGuardBuilder, Section};
use sqlx::sqlite::SqlitePoolOptions;
use std::time::Duration;
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

    let mut i: i64 = 0;
    loop {
        i += 1;
        sqlx::query("INSERT INTO users (name, age) VALUES (?, ?)")
            .bind(format!("user{i}"))
            .bind(20 + (i % 50))
            .execute(&pool)
            .await?;

        let _ = sqlx::query("SELECT id, name, age FROM users WHERE id = ?")
            .bind(i % 100 + 1)
            .fetch_optional(&pool)
            .await?;

        let q = format!("SELECT name FROM users WHERE age = {}", 20 + (i % 30));
        let _ = sqlx::query(&q).fetch_all(&pool).await?;

        let _: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users")
            .fetch_one(&pool)
            .await?;

        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}
