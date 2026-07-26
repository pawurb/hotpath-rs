//! Run with:
//!   cargo run --manifest-path crates/test-toasty/Cargo.toml --example benchmark_sql_toasty --features hotpath

use hotpath::{HotpathGuardBuilder, Section};
use std::time::{Duration, Instant};
use tracing_subscriber::prelude::*;

// Single-threaded stress test comparing Toasty SQL instrumentation overhead in one run:
// an uninstrumented baseline (no tracing subscriber installed) and the
// `hotpath::toasty_tracing_layer()` instrumented version, each hammering the same point
// lookup against an in-memory SQLite database. The delta vs baseline isolates the
// per-query cost of tracing dispatch, normalization keying, and event enqueue. Run with
// `--features hotpath` (without it the layer is a no-op). Iteration count via
// `HOTPATH_BENCH_RUNS`.

#[derive(Debug, toasty::Model)]
struct User {
    #[key]
    #[auto]
    id: uuid::Uuid,

    name: String,

    age: i64,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let runs = bench_runs();

    let mut db = toasty::Db::builder()
        .models(toasty::models!(crate::*))
        .connect("sqlite::memory:")
        .await?;
    db.push_schema().await?;
    toasty::create!(User {
        name: "user1",
        age: 21,
    })
    .exec(&mut db)
    .await?;

    // Warm the connection and statement cache so the baseline phase doesn't
    // pay one-time setup costs.
    phase(&mut db, runs / 10).await?;
    let baseline = phase(&mut db, runs).await?;

    tracing_subscriber::registry()
        .with(hotpath::toasty_tracing_layer())
        .init();
    let _guard = HotpathGuardBuilder::new("main")
        .sections(vec![Section::Sql])
        .build();

    let instrumented = phase(&mut db, runs).await?;

    report("Toasty (in-memory SQLite)", runs, baseline, instrumented);
    Ok(())
}

// Instrumented for parity with the other SQL benchmarks, but Toasty runs every
// connection as a spawned actor task and the `toasty::query` event fires
// there, outside this function's scope - the report's Source column stays
// empty on every backend.
#[hotpath::measure]
async fn phase(db: &mut toasty::Db, runs: u64) -> Result<Duration, Box<dyn std::error::Error>> {
    let start = Instant::now();
    for _ in 0..runs {
        let _ = toasty::sql::query("SELECT id, name, age FROM users WHERE age = 21")
            .exec(&mut *db)
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
