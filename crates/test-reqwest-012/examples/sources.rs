//! Demonstrates HTTP source attribution: the same endpoint requested from two
//! instrumented functions lands in two report entries, each tagged with the
//! function it was issued from. Requests sent outside any measured scope get
//! no source.
//!
//! Run with:
//!   cargo run -p test-reqwest-012 --example sources --features hotpath

use hotpath::{HotpathGuardBuilder, Section};

type Client = hotpath::wrap::reqwest::Client;

fn start_server() -> u16 {
    let server = tiny_http::Server::http("127.0.0.1:0").expect("bind test server");
    let port = server.server_addr().to_ip().expect("ip listener").port();
    std::thread::spawn(move || {
        for request in server.incoming_requests() {
            let response = tiny_http::Response::from_string("{\"ok\":true}");
            let _ = request.respond(response);
        }
    });
    port
}

#[hotpath::measure]
async fn fetch_from_a(client: &Client, port: u16) -> Result<(), Box<dyn std::error::Error>> {
    client
        .get(format!("http://127.0.0.1:{port}/users/1"))
        .send()
        .await?;
    Ok(())
}

#[hotpath::measure]
async fn fetch_from_b(client: &Client, port: u16) -> Result<(), Box<dyn std::error::Error>> {
    for id in 2..=3 {
        client
            .get(format!("http://127.0.0.1:{port}/users/{id}"))
            .send()
            .await?;
    }
    Ok(())
}

// Attribution picks the innermost instrumented function: requests issued by
// `fetch_from_a` stay attributed to it even when it runs inside another
// measured function.
#[hotpath::measure]
async fn outer(client: &Client, port: u16) -> Result<(), Box<dyn std::error::Error>> {
    fetch_from_a(client, port).await
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _guard = HotpathGuardBuilder::new("main")
        .sections(vec![Section::Http])
        .build();

    let port = start_server();
    let client: Client = hotpath::http!(reqwest::Client::new());

    fetch_from_a(&client, port).await?;
    fetch_from_b(&client, port).await?;
    outer(&client, port).await?;

    // Same endpoint outside any measured scope: separate, source-less entry.
    client
        .get(format!("http://127.0.0.1:{port}/users/9"))
        .send()
        .await?;

    println!("HTTP sources example completed");
    Ok(())
}
