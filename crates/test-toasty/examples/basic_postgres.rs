//! Same tracing-layer demo as `basic.rs`, but against a real PostgreSQL
//! server. Exercises Toasty's PostgreSQL driver, whose `$1`/`$2` positional
//! placeholders normalization merges into `?` buckets, matching the sqlite
//! output.
//!
//! Start the database first (repo-root compose file):
//!   docker compose up -d postgres
//!
//! Run with:
//!   cargo run --manifest-path crates/test-toasty/Cargo.toml --example basic_postgres --features hotpath

use hotpath::{HotpathGuardBuilder, Section};
use tracing_subscriber::prelude::*;

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
    tracing_subscriber::registry()
        .with(hotpath::toasty_tracing_layer())
        .init();

    let _guard = HotpathGuardBuilder::new("main")
        .sections(vec![Section::Sql])
        .build();

    let url = std::env::var("TOASTY_CONNECTION_URL")
        .unwrap_or_else(|_| "postgresql://hotpath:hotpath@localhost:5439/hotpath".to_string());

    let mut db = toasty::Db::builder()
        .models(toasty::models!(crate::*))
        .connect(&url)
        .await?;

    // The database persists across runs, unlike sqlite::memory:.
    let _ = toasty::sql::statement("DROP TABLE IF EXISTS users")
        .exec(&mut db)
        .await?;
    db.push_schema().await?;

    // 50 model creates, identical generated SQL -> one INSERT bucket.
    let mut ids = Vec::new();
    for i in 0..50 {
        let user = toasty::create!(User {
            name: format!("user{i}"),
            age: 20 + i,
        })
        .exec(&mut db)
        .await?;
        ids.push(user.id);
    }

    // 30 model point lookups, bind params -> one SELECT bucket.
    for id in ids.iter().take(30) {
        let _ = User::get_by_id(&mut db, id).await?;
    }

    // 20 raw queries with VARYING inline literals -> normalization merges them.
    for i in 1..=20 {
        let q = format!("SELECT name FROM users WHERE age = {}", 20 + i);
        let _ = toasty::sql::query(q).exec(&mut db).await?;
    }

    // IN-lists of different arity -> both collapse to `IN (?)`.
    let _ = toasty::sql::query("SELECT name FROM users WHERE age IN (21, 22, 23)")
        .exec(&mut db)
        .await?;
    let _ = toasty::sql::query("SELECT name FROM users WHERE age IN (24, 25, 26, 27, 28)")
        .exec(&mut db)
        .await?;

    // 10 raw aggregates -> one bucket.
    for _ in 0..10 {
        let _ = toasty::sql::query("SELECT COUNT(*) FROM users")
            .exec(&mut db)
            .await?;
    }

    // Transaction-internal queries are captured too - the event is emitted at
    // the driver level, below the transaction machinery.
    let mut tx = db.transaction().await?;
    toasty::create!(User {
        name: "in_tx",
        age: 99,
    })
    .exec(&mut tx)
    .await?;
    tx.commit().await?;

    println!("toasty postgres tracing-layer example completed!");

    // Keeps the process (and metrics server) alive so integration tests can
    // poll the HTTP endpoints mid-run.
    if let Ok(secs) = std::env::var("TEST_SLEEP_SECONDS") {
        let secs: u64 = secs.parse().unwrap_or(0);
        tokio::time::sleep(std::time::Duration::from_secs(secs)).await;
    }

    Ok(())
}
