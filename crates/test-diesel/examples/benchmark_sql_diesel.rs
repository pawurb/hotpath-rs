use diesel::prelude::*;
use diesel::sql_types::Integer;
use hotpath::{HotpathGuardBuilder, Section};
use std::time::{Duration, Instant};

// Single-threaded stress test comparing Diesel SQL instrumentation overhead in one run:
// an uninstrumented baseline (connection established before the instrumentation install,
// so it keeps diesel's default noop instrumentation) and the
// `hotpath::instrument_diesel_sql()` instrumented version, each hammering the same point
// lookup against its own in-memory SQLite database. The delta vs baseline isolates the
// per-query cost of the instrumentation callback, normalization keying, and event
// enqueue. Run with `--features hotpath` (without it the install is a no-op). Iteration
// count via `HOTPATH_BENCH_RUNS`.
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

    report("Diesel (in-memory SQLite)", runs, baseline, instrumented);
    Ok(())
}

fn setup_conn() -> Result<SqliteConnection, Box<dyn std::error::Error>> {
    let mut conn = SqliteConnection::establish(":memory:")?;
    diesel::sql_query("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT, age INTEGER)")
        .execute(&mut conn)?;
    diesel::sql_query("INSERT INTO users (name, age) VALUES ('user1', 21)").execute(&mut conn)?;
    Ok(conn)
}

// Diesel's instrumentation callback fires on the calling thread, so the
// queries are attributed to this function in the Source column.
#[hotpath::measure]
fn phase(conn: &mut SqliteConnection, runs: u64) -> Result<Duration, diesel::result::Error> {
    let start = Instant::now();
    for _ in 0..runs {
        diesel::sql_query("SELECT id, name, age FROM users WHERE id = ?")
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
        .unwrap_or(50_000)
}
