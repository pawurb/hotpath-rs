use diesel::prelude::*;
use diesel::sql_types::Integer;
use hotpath::{HotpathGuardBuilder, Section};
use std::time::{Duration, Instant};

// Same overhead benchmark as `benchmark_sql_diesel.rs`, but against a real
// PostgreSQL server, so each op includes a TCP round trip. That round trip
// dominates the per-op cost, so deltas below ~1µs are within run-to-run noise.
// Start the database first:
//   docker compose up -d postgres
// Run with `--features hotpath,pg`. Iteration count via `HOTPATH_BENCH_RUNS`.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let runs = bench_runs();

    // Each phase warms its own connection and statement cache first so neither
    // pays one-time setup costs.
    let mut conn = setup_conn()?;
    phase(&mut conn, runs / 10)?;
    let baseline = phase(&mut conn, runs)?;

    hotpath::instrument_diesel_sql();
    let _guard = HotpathGuardBuilder::new("main")
        .sections(vec![Section::Sql])
        .build();

    // Established after the install so it picks up the hotpath instrumentation.
    let mut conn = setup_conn()?;
    phase(&mut conn, runs / 10)?;
    let instrumented = phase(&mut conn, runs)?;

    report("Diesel (PostgreSQL)", runs, baseline, instrumented);
    Ok(())
}

fn setup_conn() -> Result<PgConnection, Box<dyn std::error::Error>> {
    let url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://hotpath:hotpath@localhost:5439/hotpath".to_string());
    let mut conn = PgConnection::establish(&url)?;
    // The database persists across runs and is shared with the other postgres
    // examples - use a distinct table so concurrently running binaries don't race.
    diesel::sql_query("DROP TABLE IF EXISTS diesel_bench_users").execute(&mut conn)?;
    diesel::sql_query(
        "CREATE TABLE diesel_bench_users (id SERIAL PRIMARY KEY, name TEXT, age INTEGER)",
    )
    .execute(&mut conn)?;
    diesel::sql_query("INSERT INTO diesel_bench_users (name, age) VALUES ('user1', 21)")
        .execute(&mut conn)?;
    Ok(conn)
}

// Diesel's instrumentation callback fires on the calling thread, so the
// queries are attributed to this function in the Source column.
#[hotpath::measure]
fn phase(conn: &mut PgConnection, runs: u64) -> Result<Duration, diesel::result::Error> {
    let start = Instant::now();
    for _ in 0..runs {
        diesel::sql_query("SELECT id, name, age FROM diesel_bench_users WHERE id = $1")
            .bind::<Integer, _>(1)
            .execute(conn)?;
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
