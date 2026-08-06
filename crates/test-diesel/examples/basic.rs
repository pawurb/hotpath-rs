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

struct User;

#[hotpath::measure_all]
impl User {
    fn create_table(conn: &mut SqliteConnection) -> QueryResult<usize> {
        diesel::sql_query("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT, age INTEGER)")
            .execute(conn)
    }

    fn create(conn: &mut SqliteConnection, name: &str, age: i32) -> QueryResult<usize> {
        diesel::sql_query("INSERT INTO users (name, age) VALUES (?, ?)")
            .bind::<Text, _>(name)
            .bind::<Integer, _>(age)
            .execute(conn)
    }

    fn find_by_id(conn: &mut SqliteConnection, id: i32) -> QueryResult<usize> {
        diesel::sql_query("SELECT id, name, age FROM users WHERE id = ?")
            .bind::<Integer, _>(id)
            .execute(conn)
    }

    fn find_by_age(conn: &mut SqliteConnection, age: i32) -> QueryResult<usize> {
        // Inline literal on purpose - exercises normalization of varying literals.
        diesel::sql_query(format!("SELECT name FROM users WHERE age = {age}")).execute(conn)
    }

    fn find_by_ids(conn: &mut SqliteConnection, ids: &[i32]) -> QueryResult<usize> {
        let list = ids
            .iter()
            .map(|id| id.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        diesel::sql_query(format!("SELECT * FROM users WHERE id IN ({list})")).execute(conn)
    }

    fn count(conn: &mut SqliteConnection) -> QueryResult<usize> {
        diesel::sql_query("SELECT COUNT(*) FROM users").execute(conn)
    }

    fn create_in_transaction(
        conn: &mut SqliteConnection,
        name: &str,
        age: i32,
    ) -> QueryResult<usize> {
        // Transaction-internal query is captured; BEGIN/COMMIT are not (they arrive
        // as dedicated transaction events we ignore, keeping the report queries-only).
        conn.transaction(|conn| User::create(conn, name, age))
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    hotpath::instrument_diesel_sql();

    let _guard = HotpathGuardBuilder::new("main")
        .sections(vec![Section::Sql])
        .build();

    // Established AFTER install so it picks up the default instrumentation.
    let mut conn = SqliteConnection::establish(":memory:")?;

    User::create_table(&mut conn)?;

    // 50 inserts, identical prepared text -> one bucket.
    for i in 0..50 {
        User::create(&mut conn, &format!("user{i}"), 20 + i)?;
    }

    // 30 point lookups, bind params -> one bucket.
    for i in 1..=30 {
        User::find_by_id(&mut conn, i)?;
    }

    // 20 selects with VARYING inline literals -> normalization merges them.
    for i in 1..=20 {
        User::find_by_age(&mut conn, 20 + i)?;
    }

    // IN-lists of different arity -> both collapse to `IN (?)`.
    User::find_by_ids(&mut conn, &[1, 2, 3])?;
    User::find_by_ids(&mut conn, &[4, 5, 6, 7, 8])?;

    // 10 aggregates -> one bucket.
    for _ in 0..10 {
        User::count(&mut conn)?;
    }

    User::create_in_transaction(&mut conn, "in_tx", 99)?;

    println!("Diesel instrumentation example completed!");
    Ok(())
}
