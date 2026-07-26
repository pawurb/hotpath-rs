use hotpath::{HotpathGuardBuilder, Section};
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::SqlitePool;
use std::time::{Duration, Instant};
use tracing_subscriber::prelude::*;

// Single-threaded stress test comparing sqlx SQL instrumentation overhead in one run: an
// uninstrumented baseline (no tracing subscriber installed) and the
// `hotpath::sqlx_tracing_layer()` instrumented version, each hammering the same point
// lookup against an in-memory SQLite database. The delta vs baseline isolates the
// per-query cost of tracing dispatch, normalization keying, and event enqueue. Run with
// `--features hotpath` (without it the layer is a no-op). Iteration count via
// `HOTPATH_BENCH_RUNS`.
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

    report("sqlx (in-memory SQLite)", runs, baseline, instrumented);
    Ok(())
}

async fn setup_pool() -> Result<SqlitePool, sqlx::Error> {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await?;
    sqlx::query("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT, age INTEGER)")
        .execute(&pool)
        .await?;
    sqlx::query("INSERT INTO users (name, age) VALUES ('user1', 21)")
        .execute(&pool)
        .await?;
    Ok(pool)
}

// Instrumented so queries carry a source, though with the sqlite driver the
// `sqlx::query` event fires on a dedicated connection worker thread, outside
// this function's scope - the report's Source column stays empty. Run the
// postgres variant to see attribution.
#[hotpath::measure]
async fn phase(pool: &SqlitePool, runs: u64) -> Result<Duration, sqlx::Error> {
    let start = Instant::now();
    for _ in 0..runs {
        let _ = sqlx::query("SELECT id, name, age FROM users WHERE id = ?")
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
        .unwrap_or(50_000)
}
