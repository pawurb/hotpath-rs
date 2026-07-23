//! Demonstrates `hotpath::toasty_tracing_layer()` capturing every Toasty ORM
//! query via a `tracing` layer - no driver wrapping, no application type
//! changes. Toasty's drivers emit one `toasty::query` event per physical
//! database operation (model-generated SQL and raw SQL alike), and the layer
//! forwards each one to the shared hotpath SQL pipeline.
//!
//! Run with:
//!   cargo run --manifest-path crates/test-toasty/Cargo.toml --example basic --features hotpath

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

    let mut db = toasty::Db::builder()
        .models(toasty::models!(crate::*))
        .connect("sqlite::memory:")
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

    println!("toasty tracing-layer example completed!");

    // Keeps the process (and metrics server) alive so integration tests can
    // poll the HTTP endpoints mid-run.
    if let Ok(secs) = std::env::var("TEST_SLEEP_SECONDS") {
        let secs: u64 = secs.parse().unwrap_or(0);
        tokio::time::sleep(std::time::Duration::from_secs(secs)).await;
    }

    Ok(())
}
