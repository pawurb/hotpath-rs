//! Same `hotpath::instrument_diesel_sql()` demo as `basic.rs`, but against a
//! real PostgreSQL server. Exercises the PostgreSQL positional placeholder
//! syntax (`$1`, `$2`) which normalization merges into `?` buckets, matching
//! the sqlite output.
//!
//! Start the database first:
//!   docker compose up -d postgres
//!
//! Run with:
//!   cargo run -p test-diesel --example basic_postgres --features hotpath,pg

use diesel::prelude::*;
use diesel::sql_types::{Integer, Text};
use hotpath::{HotpathGuardBuilder, Section};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    hotpath::instrument_diesel_sql();

    let _guard = HotpathGuardBuilder::new("main")
        .sections(vec![Section::Sql])
        .build();

    let url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://hotpath:hotpath@localhost:5439/hotpath".to_string());

    // Established AFTER install so it picks up the default instrumentation.
    let mut conn = PgConnection::establish(&url)?;

    // The database persists across runs (unlike sqlite ":memory:") and is
    // shared with the sqlx postgres examples, which own the `users` table -
    // use a distinct table so concurrently running test binaries don't race.
    diesel::sql_query("DROP TABLE IF EXISTS diesel_users").execute(&mut conn)?;
    diesel::sql_query("CREATE TABLE diesel_users (id SERIAL PRIMARY KEY, name TEXT, age INTEGER)")
        .execute(&mut conn)?;

    // 50 inserts, identical prepared text -> one bucket ($1/$2 normalize to ?).
    for i in 0..50 {
        diesel::sql_query("INSERT INTO diesel_users (name, age) VALUES ($1, $2)")
            .bind::<Text, _>(format!("user{i}"))
            .bind::<Integer, _>(20 + i)
            .execute(&mut conn)?;
    }

    // 30 point lookups, bind params -> one bucket.
    for i in 1..=30 {
        diesel::sql_query("SELECT id, name, age FROM diesel_users WHERE id = $1")
            .bind::<Integer, _>(i)
            .execute(&mut conn)?;
    }

    // 20 selects with VARYING inline literals -> normalization merges them.
    for i in 1..=20 {
        let q = format!("SELECT name FROM diesel_users WHERE age = {}", 20 + i);
        diesel::sql_query(q).execute(&mut conn)?;
    }

    // IN-lists of different arity -> both collapse to `IN (?)`.
    diesel::sql_query("SELECT * FROM diesel_users WHERE id IN (1, 2, 3)").execute(&mut conn)?;
    diesel::sql_query("SELECT * FROM diesel_users WHERE id IN (4, 5, 6, 7, 8)")
        .execute(&mut conn)?;

    // 10 aggregates -> one bucket.
    for _ in 0..10 {
        diesel::sql_query("SELECT COUNT(*) FROM diesel_users").execute(&mut conn)?;
    }

    // Transaction-internal query is captured; BEGIN/COMMIT are not (they arrive
    // as dedicated transaction events we ignore, keeping the report queries-only).
    conn.transaction::<_, diesel::result::Error, _>(|conn| {
        diesel::sql_query("INSERT INTO diesel_users (name, age) VALUES ($1, $2)")
            .bind::<Text, _>("in_tx")
            .bind::<Integer, _>(99)
            .execute(conn)?;
        Ok(())
    })?;

    println!("Diesel postgres instrumentation example completed!");
    Ok(())
}
