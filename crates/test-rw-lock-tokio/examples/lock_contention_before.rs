//! Demonstrates the long-held-guard antipattern: a writer task acquires the
//! RwLock write guard first and only then performs an HTTP request, holding
//! the lock across the whole network round trip. A concurrent reader task
//! is blocked for the entire duration of each refresh, which shows up as
//! high read wait times in the rw_locks report. Compare with the
//! `lock_contention_after` example.
//!
//! Run with:
//!   cargo run -p test-rw-lock-tokio --example lock_contention_before --features hotpath

use std::sync::Arc;
use std::time::Duration;

use hotpath::{HotpathGuardBuilder, Section};

type Cache = Arc<hotpath::wrap::tokio::sync::RwLock<Vec<String>>>;

// Endpoint responding after a 1 second delay, to simulate a slow upstream API.
const URL: &str = "https://postman-echo.com/delay/1";

#[hotpath::measure]
async fn refresh_quotes(
    client: &reqwest::Client,
    cache: &Cache,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Antipattern: the write guard is held across the whole HTTP round trip,
    // so every reader is blocked until the response arrives.
    let mut quotes = cache.write().await;
    let body = client.get(URL).send().await?.text().await?;
    quotes.push(body);
    Ok(())
}

#[hotpath::measure]
async fn latest_quote(cache: &Cache) -> Option<usize> {
    let quotes = cache.read().await;
    quotes.last().map(|q| q.len())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let _guard = HotpathGuardBuilder::new("main")
        .sections(vec![Section::FunctionsTiming, Section::RwLocks])
        .build();

    let cache: Cache = Arc::new(hotpath::rw_lock!(
        tokio::sync::RwLock::new(Vec::new()),
        label = "quotes_cache"
    ));
    let client = reqwest::Client::new();

    let writer = tokio::spawn({
        let cache = cache.clone();
        async move {
            for _ in 0..3 {
                refresh_quotes(&client, &cache).await.unwrap();
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        }
    });

    let reader = tokio::spawn({
        let cache = cache.clone();
        async move {
            for _ in 0..50 {
                let _ = latest_quote(&cache).await;
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        }
    });

    let (writer_res, reader_res) = tokio::join!(writer, reader);
    writer_res?;
    reader_res?;

    println!("Cached quotes: {}", cache.read().await.len());
    Ok(())
}
