use futures_util::stream::{self, StreamExt};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

pub fn init() {
    spawn_tokio_demo();
    spawn_rw_locks();
    spawn_mutexes();
    spawn_io();
    spawn_channels();
    spawn_std_channel();
    spawn_aggregated_channels();
    #[cfg(feature = "demo")]
    spawn_sqlx_sql();
    #[cfg(feature = "demo")]
    spawn_diesel_sql();
    #[cfg(feature = "demo")]
    spawn_http();
}

fn spawn_channels() {
    // Wrap channel: tracks exact send->receive latency. The consumer outpaces the
    // producer, so the bounded channel stays near empty and each message is processed
    // as soon as it arrives - a healthy channel that keeps up with its load.
    let (tx, rx) = hotpath::channel!(crossbeam_channel::bounded::<u64>(8), label = "demo-jobs");

    thread::spawn(move || {
        let mut i = 0u64;
        while tx.send(i).is_ok() {
            i += 1;
            thread::sleep(Duration::from_millis(40));
        }
    });

    thread::spawn(move || {
        while let Ok(job) = rx.recv() {
            std::hint::black_box(job);
            thread::sleep(Duration::from_millis(15));
        }
    });
}

fn spawn_aggregated_channels() {
    // Default (aggregated) mode: a fresh short-lived channel per iteration at
    // one call site, mirroring request-scoped channels in a server. All
    // instances collapse into a single entry whose Inst count keeps climbing
    // while counts and latency accumulate across them.
    thread::spawn(|| {
        let mut i = 0u64;
        loop {
            let (tx, rx) = hotpath::channel!(
                crossbeam_channel::bounded::<u64>(4),
                label = "demo-per-request"
            );
            for _ in 0..3 {
                let _ = tx.send(i);
                i += 1;
            }
            drop(tx);
            while let Ok(job) = rx.recv() {
                std::hint::black_box(job);
            }
            thread::sleep(Duration::from_millis(200));
        }
    });
}

fn spawn_std_channel() {
    // Endpoint-wrapped std::sync::mpsc channel. Bounded wrappers need an explicit
    // `capacity` (std doesn't expose it). The producer outpaces the consumer so the
    // self-tracked queue depth climbs to the bound.
    let (tx, rx) = hotpath::channel!(
        std::sync::mpsc::sync_channel::<u64>(8),
        capacity = 8,
        label = "demo-std-jobs"
    );

    thread::spawn(move || {
        let mut i = 0u64;
        while tx.send(i).is_ok() {
            i += 1;
            thread::sleep(Duration::from_millis(10));
        }
    });

    thread::spawn(move || {
        while let Ok(job) = rx.recv() {
            std::hint::black_box(job);
            thread::sleep(Duration::from_millis(30));
        }
    });
}

fn spawn_io() {
    // In-memory Cursor exercised in a loop; shows read/write throughput on the
    // Bytes sub-tab.
    thread::spawn(|| {
        use std::io::{Read, Write};

        let mut io = hotpath::io!(std::io::Cursor::new(Vec::new()));
        let payload = [7u8; 256];
        let mut buf = [0u8; 64];
        loop {
            io.set_position(0);
            let _ = io.write_all(&payload);
            let _ = io.flush();
            io.set_position(0);
            while let Ok(n) = io.read(&mut buf) {
                if n == 0 {
                    break;
                }
            }
            thread::sleep(Duration::from_millis(50));
        }
    });
}

fn spawn_mutexes() {
    let lock = Arc::new(hotpath::mutex!(
        std::sync::Mutex::new(0u64),
        label = "demo-mutex"
    ));

    // A few contending threads holding the lock for varying durations.
    for delay_ms in [60u64, 90, 130] {
        let lock = Arc::clone(&lock);
        thread::spawn(move || loop {
            {
                let mut v = lock.lock().unwrap();
                *v += 1;
                thread::sleep(Duration::from_millis(8));
            }
            thread::sleep(Duration::from_millis(delay_ms));
        });
    }
}

fn spawn_rw_locks() {
    let lock = Arc::new(hotpath::rw_lock!(
        std::sync::RwLock::new(0u64),
        label = "demo-counter"
    ));

    // Writer: bumps the counter periodically, holding the write lock briefly.
    let writer = Arc::clone(&lock);
    thread::spawn(move || loop {
        {
            let mut w = writer.write().unwrap();
            *w += 1;
            thread::sleep(Duration::from_millis(5));
        }
        thread::sleep(Duration::from_millis(120));
    });

    // Readers: a few threads sampling the counter with varying hold times.
    for delay_ms in [40u64, 70, 110] {
        let reader = Arc::clone(&lock);
        thread::spawn(move || loop {
            {
                let r = reader.read().unwrap();
                std::hint::black_box(*r);
                thread::sleep(Duration::from_millis(2));
            }
            thread::sleep(Duration::from_millis(delay_ms));
        });
    }

    // Second lock: write-heavy with longer holds than the counter.
    let config = Arc::new(hotpath::rw_lock!(
        std::sync::RwLock::new(0u64),
        label = "demo-config"
    ));

    let cfg_writer = Arc::clone(&config);
    thread::spawn(move || loop {
        {
            let mut w = cfg_writer.write().unwrap();
            *w += 1;
            thread::sleep(Duration::from_millis(15));
        }
        thread::sleep(Duration::from_millis(50));
    });

    let cfg_reader = Arc::clone(&config);
    thread::spawn(move || loop {
        {
            let r = cfg_reader.read().unwrap();
            std::hint::black_box(*r);
            thread::sleep(Duration::from_millis(1));
        }
        thread::sleep(Duration::from_millis(200));
    });
}

#[cfg(feature = "demo")]
fn spawn_sqlx_sql() {
    use sqlx::sqlite::SqlitePoolOptions;
    use tracing_subscriber::prelude::*;

    thread::spawn(|| {
        // Route sqlx's per-query `sqlx::query` tracing events into hotpath's SQL subsystem.
        tracing_subscriber::registry()
            .with(hotpath::sqlx_tracing_layer())
            .init();

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("Failed to create tokio runtime");

        rt.block_on(async {
            let pool = SqlitePoolOptions::new()
                .max_connections(2)
                .connect("sqlite::memory:")
                .await
                .expect("Failed to open in-memory sqlite pool");

            sqlx::query("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT, age INTEGER)")
                .execute(&pool)
                .await
                .expect("Failed to create demo table");

            let mut i: i64 = 0;
            loop {
                i += 1;

                let _ = sqlx::query("INSERT INTO users (name, age) VALUES (?, ?)")
                    .bind(format!("user{i}"))
                    .bind(20 + (i % 50))
                    .execute(&pool)
                    .await;

                let _ = sqlx::query("SELECT id, name, age FROM users WHERE id = ?")
                    .bind(i % 100 + 1)
                    .fetch_optional(&pool)
                    .await;

                // Varying inline literals collapse into one normalized bucket.
                let q = format!("SELECT name FROM users WHERE age = {}", 20 + (i % 30));
                let _ = sqlx::query(sqlx::AssertSqlSafe(q)).fetch_all(&pool).await;

                let _: Result<(i64,), _> = sqlx::query_as("SELECT COUNT(*) FROM users")
                    .fetch_one(&pool)
                    .await;

                sleep_ms(120).await;
            }
        });
    });
}

#[cfg(feature = "demo")]
fn spawn_diesel_sql() {
    use diesel::prelude::*;

    // Capture Diesel queries via connection::Instrumentation into the same SQL
    // subsystem the sqlx layer feeds - both ORMs share one report.
    hotpath::instrument_diesel_sql();

    thread::spawn(|| {
        // Established after instrument_diesel_sql() so it picks up the instrumentation.
        let mut conn = SqliteConnection::establish(":memory:")
            .expect("Failed to open in-memory sqlite connection");

        diesel::sql_query("CREATE TABLE orders (id INTEGER PRIMARY KEY, sku TEXT, qty INTEGER)")
            .execute(&mut conn)
            .expect("Failed to create demo table");

        let mut i: i64 = 0;
        loop {
            i += 1;

            insert_order(&mut conn, i);
            lookup_order(&mut conn, i);

            // Issued outside any measured scope, so these buckets keep an
            // empty Source. Varying inline literals collapse into one
            // normalized bucket.
            let q = format!("SELECT sku FROM orders WHERE qty = {}", i % 20);
            let _ = diesel::sql_query(q).execute(&mut conn);

            let _ = diesel::sql_query("SELECT COUNT(*) FROM orders").execute(&mut conn);

            thread::sleep(Duration::from_millis(150));
        }
    });
}

// Measured issuers so the SQL tab's Source column shows per-function
// attribution. Diesel executes on the calling thread, so its queries
// attribute correctly (the sqlx-sqlite demo can't - that driver runs
// statements on a connection worker thread).
#[cfg(feature = "demo")]
#[hotpath::measure]
fn insert_order(conn: &mut diesel::SqliteConnection, i: i64) {
    use diesel::prelude::*;
    use diesel::sql_types::{Integer, Text};

    let _ = diesel::sql_query("INSERT INTO orders (sku, qty) VALUES (?, ?)")
        .bind::<Text, _>(format!("sku{i}"))
        .bind::<Integer, _>((i % 20) as i32)
        .execute(conn);
}

#[cfg(feature = "demo")]
#[hotpath::measure]
fn lookup_order(conn: &mut diesel::SqliteConnection, i: i64) {
    use diesel::prelude::*;
    use diesel::sql_types::Integer;

    let _ = diesel::sql_query("SELECT id, sku, qty FROM orders WHERE id = ?")
        .bind::<Integer, _>((i % 100 + 1) as i32)
        .execute(conn);
}

#[cfg(feature = "demo")]
fn spawn_http() {
    // Local axum server wrapped in `hotpath::axum!`, so one request loop feeds
    // both the HTTP (client) and Server subtabs without network access.
    // Per-route delays give the endpoints distinct profiles.
    let (port_tx, port_rx) = std::sync::mpsc::channel::<u16>();
    thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("Failed to create tokio runtime");

        rt.block_on(async move {
            use axum::routing::get;

            let app = hotpath::axum!(axum::Router::new()
                .route("/users/{id}", get(|| delayed_ok(5)))
                .route("/search", get(|| delayed_ok(40)))
                .route("/slow", get(|| delayed_ok(250)))
                .fallback(|| async {
                    tokio::time::sleep(Duration::from_millis(2)).await;
                    axum::http::StatusCode::NOT_FOUND
                }));
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
                .await
                .expect("Failed to bind demo http server");
            let port = listener.local_addr().expect("local addr").port();
            let _ = port_tx.send(port);
            axum::serve(listener, app).await.expect("demo axum server");
        });
    });
    let port = port_rx.recv().expect("demo http server port");

    thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("Failed to create tokio runtime");

        rt.block_on(async move {
            let client = hotpath::http!(reqwest::Client::new());
            let base = format!("http://127.0.0.1:{port}");
            let mut i: u64 = 0;
            loop {
                i += 1;

                fetch_user(&client, &base, i).await;

                if i.is_multiple_of(3) {
                    run_search(&client, &base, i).await;
                }

                if i.is_multiple_of(7) {
                    fetch_slow(&client, &base).await;
                }

                // 404s feed the Errors column.
                if i.is_multiple_of(11) {
                    fetch_missing(&client, &base, i).await;
                }

                sleep_ms(120).await;
            }
        });
    });
}

#[cfg(feature = "demo")]
async fn delayed_ok(delay_ms: u64) -> &'static str {
    tokio::time::sleep(Duration::from_millis(delay_ms)).await;
    "{\"ok\":true}"
}

#[cfg(feature = "demo")]
type DemoHttpClient = hotpath::wrap::reqwest::Client;

// Measured issuers so the HTTP tab's Source column shows per-function
// attribution for the shared endpoints.
#[cfg(feature = "demo")]
#[hotpath::measure]
async fn fetch_user(client: &DemoHttpClient, base: &str, i: u64) {
    // Varying ids and query strings collapse into one normalized bucket.
    let _ = client
        .get(format!("{base}/users/{}", i % 100 + 1))
        .send()
        .await;
}

#[cfg(feature = "demo")]
#[hotpath::measure]
async fn run_search(client: &DemoHttpClient, base: &str, i: u64) {
    let _ = client.get(format!("{base}/search?q=term{i}")).send().await;
}

#[cfg(feature = "demo")]
#[hotpath::measure]
async fn fetch_slow(client: &DemoHttpClient, base: &str) {
    let _ = client.get(format!("{base}/slow")).send().await;
}

#[cfg(feature = "demo")]
#[hotpath::measure]
async fn fetch_missing(client: &DemoHttpClient, base: &str, i: u64) {
    let _ = client.get(format!("{base}/missing/{i}")).send().await;
}

async fn sleep_ms(ms: u64) {
    let _ = tokio::task::spawn_blocking(move || {
        thread::sleep(Duration::from_millis(ms));
    })
    .await;
}

fn spawn_tokio_demo() {
    thread::spawn(|| {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("Failed to create tokio runtime");

        rt.block_on(async {
            spawn_streams().await;
            std::future::pending::<()>().await;
        });
    });
}

async fn spawn_streams() {
    // Fast number stream
    let stream1 = hotpath::stream!(
        stream::iter(0u64..).then(|i| async move {
            sleep_ms(80).await;
            i
        }),
        label = "demo-number-stream",
        log = true
    );

    tokio::spawn(async move {
        let mut stream = Box::pin(stream1);
        while let Some(value) = stream.next().await {
            hotpath::val!("stream_number").set(&value);
            hotpath::gauge!("stream_value").set(value);
            std::hint::black_box(value);
        }
    });

    // Text stream with slower consumption
    let texts = vec!["hello", "world", "from", "demo", "streams"];
    let stream2 = hotpath::stream!(
        stream::iter(texts.into_iter().cycle()).then(|s| async move {
            sleep_ms(200).await;
            s
        }),
        label = "demo-text-stream",
        log = true
    );

    tokio::spawn(async move {
        let mut stream = Box::pin(stream2);
        while let Some(text) = stream.next().await {
            std::hint::black_box(text);
        }
    });

    // Repeat stream
    let stream3 = hotpath::stream!(
        stream::repeat(42u64).then(|v| async move {
            sleep_ms(150).await;
            v
        }),
        label = "demo-repeat-stream"
    );

    tokio::spawn(async move {
        let mut stream = Box::pin(stream3);
        while let Some(value) = stream.next().await {
            std::hint::black_box(value);
        }
    });
}
