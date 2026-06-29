//! Demonstrates `hotpath::instrument_diesel_sql()` capturing every
//! Diesel query via `diesel::connection::Instrumentation` - no pool wrapping, no
//! application type changes. Queries feed the same SQL pipeline as the sqlx
//! tracing layer, so they normalize and report identically.
//!
//! Run with:
//!   cargo run -p test-diesel --example basic --features hotpath

use diesel::prelude::*;
use diesel::sql_types::{Integer, Text};
use hotpath::{HotpathGuardBuilder, Section};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    hotpath::instrument_diesel_sql();

    let _guard = HotpathGuardBuilder::new("main")
        .sections(vec![Section::Sql])
        .build();

    // Established AFTER install so it picks up the default instrumentation.
    let mut conn = SqliteConnection::establish(":memory:")?;

    diesel::sql_query("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT, age INTEGER)")
        .execute(&mut conn)?;

    // 50 inserts, identical prepared text -> one bucket.
    for i in 0..50 {
        diesel::sql_query("INSERT INTO users (name, age) VALUES (?, ?)")
            .bind::<Text, _>(format!("user{i}"))
            .bind::<Integer, _>(20 + i)
            .execute(&mut conn)?;
    }

    // 30 point lookups, bind params -> one bucket.
    for i in 1..=30 {
        diesel::sql_query("SELECT id, name, age FROM users WHERE id = ?")
            .bind::<Integer, _>(i)
            .execute(&mut conn)?;
    }

    // 20 selects with VARYING inline literals -> normalization merges them.
    for i in 1..=20 {
        let q = format!("SELECT name FROM users WHERE age = {}", 20 + i);
        diesel::sql_query(q).execute(&mut conn)?;
    }

    // IN-lists of different arity -> both collapse to `IN (?)`.
    diesel::sql_query("SELECT * FROM users WHERE id IN (1, 2, 3)").execute(&mut conn)?;
    diesel::sql_query("SELECT * FROM users WHERE id IN (4, 5, 6, 7, 8)").execute(&mut conn)?;

    // 10 aggregates -> one bucket.
    for _ in 0..10 {
        diesel::sql_query("SELECT COUNT(*) FROM users").execute(&mut conn)?;
    }

    // Transaction-internal query is captured; BEGIN/COMMIT are not (they arrive
    // as dedicated transaction events we ignore, keeping the report queries-only).
    conn.transaction::<_, diesel::result::Error, _>(|conn| {
        diesel::sql_query("INSERT INTO users (name, age) VALUES (?, ?)")
            .bind::<Text, _>("in_tx")
            .bind::<Integer, _>(99)
            .execute(conn)?;
        Ok(())
    })?;

    println!("Diesel instrumentation example completed!");
    Ok(())
}
