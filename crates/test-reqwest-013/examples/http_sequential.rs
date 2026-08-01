//! Demonstrates the sequential-I/O antipattern: three independent HTTP
//! requests awaited one after another inside an instrumented function. All
//! three land in the HTTP report attributed to `get_dashboard`, and the
//! function's total time is roughly the sum of the three request latencies.
//! Compare with the `http_parallel` example.
//!
//! Run with:
//!   cargo run -p test-reqwest-013 --example http_sequential --features hotpath

use hotpath::{HotpathGuardBuilder, Section};

type Client = hotpath::wrap::reqwest::Client;

// Each endpoint lives on a different domain, so no request can reuse another's
// pooled connection: all three pay their own DNS + TCP + TLS setup.
async fn fetch(client: &Client, url: &str) -> Result<String, Box<dyn std::error::Error>> {
    Ok(client.get(url).send().await?.text().await?)
}

#[hotpath::measure]
async fn get_dashboard(client: &Client) -> Result<(), Box<dyn std::error::Error>> {
    let user = fetch(client, "https://jsonplaceholder.typicode.com/users/1").await?;
    let posts = fetch(client, "https://dummyjson.com/posts/1").await?;
    let comments = fetch(client, "https://postman-echo.com/get").await?;

    println!(
        "fetched {} + {} + {} bytes sequentially",
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
