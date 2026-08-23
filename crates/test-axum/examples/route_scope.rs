//! Demonstrates route scoping: SQL queries and outbound HTTP requests issued
//! while an axum handler runs are attributed to the matched route template in
//! the `Route` column of the SQL and HTTP sections, alongside the `Source`
//! column. The same statement executed under two routes yields two entries,
//! which makes per-route query counts (N+1 detection) visible. The `server`
//! section derives `SQL/req` and `HTTP/req` per route from that attribution.
//!
//! Run with:
//!   cargo run -p test-axum --example route_scope --features hotpath
//!
//! Disable route attribution with `HOTPATH_ROUTE_SCOPE=0` (or
//! `HotpathGuardBuilder::route_scope(false)`): entries then collapse to
//! `(source, query)` as if the axum layer were not installed.
//!
//! Diesel's sqlite connection runs queries on the calling thread, so the
//! handler's route and caller context are visible to the SQL instrumentation
//! (async sqlx sqlite would execute them on its own connection worker thread).

use axum::extract::{Path, State};
use axum::routing::get;
use axum::Router;
use diesel::prelude::*;
use diesel::sql_types::{Integer, Text};
use hotpath::{HotpathGuardBuilder, Section};
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Clone)]
struct AppState {
    conn: Arc<Mutex<SqliteConnection>>,
    client: hotpath::wrap::reqwest::Client,
    base: String,
}

#[hotpath::measure]
fn load_user(conn: &mut SqliteConnection, id: i32) -> usize {
    diesel::sql_query("SELECT id, name FROM users WHERE id = ?")
        .bind::<Integer, _>(id)
        .execute(conn)
        .expect("load_user")
}

#[hotpath::measure]
fn count_users(conn: &mut SqliteConnection) -> usize {
    diesel::sql_query("SELECT COUNT(*) FROM users")
        .execute(conn)
        .expect("count_users")
}

async fn get_user(State(state): State<AppState>, Path(id): Path<i32>) -> String {
    let rows = load_user(&mut state.conn.lock().unwrap(), id);
    format!("user {id}: {rows}")
}

// Runs the same statement as GET /users/{id} plus a second query and an
// outbound request to the server's own /users/{id}: all land under the
// GET /profiles/{id} route, so the server section reports 2 SQL/req and
// 1 HTTP/req for it.
async fn get_profile(State(state): State<AppState>, Path(id): Path<i32>) -> String {
    let (rows, total) = {
        let mut conn = state.conn.lock().unwrap();
        (load_user(&mut conn, id), count_users(&mut conn))
    };
    let resp = state
        .client
        .get(format!("{}/users/{id}", state.base))
        .send()
        .await
        .expect("GET /users/{id}");
    format!("profile {id}: {rows}/{total} {}", resp.status())
}

// Queries outside any handler carry no route.
fn seed(conn: &mut SqliteConnection) {
    diesel::sql_query("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT)")
        .execute(conn)
        .expect("create table");
    for i in 1..=3 {
        diesel::sql_query("INSERT INTO users (name) VALUES (?)")
            .bind::<Text, _>(format!("user{i}"))
            .execute(conn)
            .expect("insert");
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    hotpath::instrument_diesel_sql();

    let _guard = HotpathGuardBuilder::new("main")
        .sections(vec![Section::Server, Section::Sql, Section::Http])
        .build();

    let mut conn = SqliteConnection::establish(":memory:")?;
    seed(&mut conn);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();
    let base = format!("http://127.0.0.1:{port}");

    let state = AppState {
        conn: Arc::new(Mutex::new(conn)),
        client: hotpath::http!(reqwest::Client::new()),
        base: base.clone(),
    };
    let app = hotpath::axum!(Router::new()
        .route("/users/{id}", get(get_user))
        .route("/profiles/{id}", get(get_profile))
        .with_state(state));
    tokio::spawn(async move {
        axum::serve(listener, app).await.expect("axum server");
    });

    // Plain client (not wrapped) so only the in-handler requests count as HTTP.
    let client = reqwest::Client::new();
    for id in 1..=2 {
        client.get(format!("{base}/users/{id}")).send().await?;
    }
    for id in 1..=3 {
        client.get(format!("{base}/profiles/{id}")).send().await?;
    }
    // Unmatched request: no route scope, so no SQL/HTTP attribution.
    client.get(format!("{base}/missing")).send().await?;

    if let Ok(secs) = std::env::var("TEST_SLEEP_SECONDS") {
        if let Ok(secs) = secs.parse::<u64>() {
            tokio::time::sleep(Duration::from_secs(secs)).await;
        }
    }

    println!("axum route scope example completed");
    Ok(())
}
