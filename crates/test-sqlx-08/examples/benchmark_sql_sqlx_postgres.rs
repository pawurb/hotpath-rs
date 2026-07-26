use hotpath::{HotpathGuardBuilder, Section};
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use std::time::{Duration, Instant};
use tracing_subscriber::prelude::*;

// Same overhead benchmark as `benchmark_sql_sqlx.rs`, but against a real
// PostgreSQL server, so each op includes a TCP round trip. That round trip
// dominates the per-op cost, so deltas below ~1µs are within run-to-run noise.
// Start the database first:
//   docker compose up -d postgres
// Run with `--features hotpath`. Iteration count via `HOTPATH_BENCH_RUNS`.
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let runs = bench_runs();

    let pool = setup_pool().await?;
    // Warm the connection and statement cache so the baseline phase doesn't
    // pay one-time setup costs.
    phase(&pool, runs / 10).await?;
    let baseline = phase(&pool, runs).await?;

    tracing_subscriber::registry()
        .with(hotpath::sqlx_tracing_layer())
        .init();
    let _guard = HotpathGuardBuilder::new("main")
        .sections(vec![Section::Sql])
        .build();

    let instrumented = phase(&pool, runs).await?;

    report("sqlx (PostgreSQL)", runs, baseline, instrumented);
    Ok(())
}

async fn setup_pool() -> Result<PgPool, sqlx::Error> {
    let url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://hotpath:hotpath@localhost:5439/hotpath".to_string());
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(&url)
        .await?;
    // The database persists across runs and is shared with the other postgres
    // examples - use a distinct table so concurrently running binaries don't race.
    sqlx::query("DROP TABLE IF EXISTS sqlx_bench_users")
        .execute(&pool)
        .await?;
    sqlx::query("CREATE TABLE sqlx_bench_users (id SERIAL PRIMARY KEY, name TEXT, age INTEGER)")
        .execute(&pool)
        .await?;
    sqlx::query("INSERT INTO sqlx_bench_users (name, age) VALUES ('user1', 21)")
        .execute(&pool)
        .await?;
    Ok(pool)
}

// The postgres driver executes queries on the calling task, so the emitted
// `sqlx::query` events are attributed to this function in the Source column.
#[hotpath::measure]
async fn phase(pool: &PgPool, runs: u64) -> Result<Duration, sqlx::Error> {
    let start = Instant::now();
    for _ in 0..runs {
        let _ = sqlx::query("SELECT id, name, age FROM sqlx_bench_users WHERE id = $1")
            .bind(1)
            .fetch_optional(pool)
            .await?;
    }
    Ok(start.elapsed())
}

fn report(name: &str, runs: u64, baseline: Duration, instrumented: Duration) {
    let per = |d: Duration| d.as_nanos() as f64 / runs as f64;
    let b = per(baseline);
    let ins = per(instrumented);
    println!("\n{name}: {runs} queries per mode");
    println!("  baseline (raw)  {b:>8.1} ns/op");
    println!(
        "  instrumented    {ins:>8.1} ns/op  ({:+.1} ns/op vs baseline)",
        ins - b
    );
}

fn bench_runs() -> u64 {
    std::env::var("HOTPATH_BENCH_RUNS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(10_000)
}
