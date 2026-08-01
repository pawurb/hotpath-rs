//! Fix for the sequential-I/O antipattern shown in the `http_sequential`
//! example: the same three independent HTTP requests run concurrently via
//! `tokio::try_join!`. The requests are still attributed to `get_dashboard`
//! (they are polled inside its measured scope), but the function's total time
//! now tracks the slowest request instead of the sum of all three.
//!
//! Run with:
//!   cargo run -p test-reqwest-013 --example http_parallel --features hotpath

use hotpath::{HotpathGuardBuilder, Section};

type Client = hotpath::wrap::reqwest::Client;

// Each endpoint lives on a different domain, so no request can reuse another's
// pooled connection: all three pay their own DNS + TCP + TLS setup.
async fn fetch(client: &Client, url: &str) -> Result<String, Box<dyn std::error::Error>> {
    Ok(client.get(url).send().await?.text().await?)
}

#[hotpath::measure]
async fn get_dashboard(client: &Client) -> Result<(), Box<dyn std::error::Error>> {
    let (user, posts, comments) = tokio::try_join!(
        fetch(client, "https://jsonplaceholder.typicode.com/users/1"),
        fetch(client, "https://dummyjson.com/posts/1"),
        fetch(client, "https://postman-echo.com/get"),
    )?;

    println!(
        "fetched {} + {} + {} bytes in parallel",
        user.len(),
        posts.len(),
        comments.len()
    );
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _guard = HotpathGuardBuilder::new("main")
        .sections(vec![Section::FunctionsTiming, Section::Http])
        .build();

    let client: Client = hotpath::http!(reqwest::Client::new());

    get_dashboard(&client).await?;

    Ok(())
}
