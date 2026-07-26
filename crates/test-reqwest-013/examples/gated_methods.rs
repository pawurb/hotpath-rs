//! Demonstrates the feature-gated `RequestBuilder` methods (`json`, `query`,
//! `form`) that `reqwest-middleware` 0.5 compiles only behind its own feature
//! flags. This crate enables `json`/`query`/`form` on its reqwest dependency,
//! so without `--features hotpath` this example compiles and runs against the
//! raw `reqwest::Client`. With `--features hotpath`, `hotpath::http!` swaps in
//! `ClientWithMiddleware`, and unless the `hotpath` crate forwards
//! `reqwest-middleware-05/json`, `/query`, and `/form`, every call site below
//! fails with "method not found".
//!
//! Run with:
//!   cargo run -p test-reqwest-013 --example gated_methods
//!   cargo run -p test-reqwest-013 --example gated_methods --features hotpath

use hotpath::{HotpathGuardBuilder, Section};
use std::collections::HashMap;

fn start_server() -> u16 {
    let server = tiny_http::Server::http("127.0.0.1:0").expect("bind test server");
    let port = server.server_addr().to_ip().expect("ip listener").port();
    std::thread::spawn(move || {
        for request in server.incoming_requests() {
            let _ = request.respond(tiny_http::Response::from_string("{\"ok\":true}"));
        }
    });
    port
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _guard = HotpathGuardBuilder::new("main")
        .sections(vec![Section::Http])
        .build();

    let port = start_server();
    let client: hotpath::wrap::reqwest::Client = hotpath::http!(reqwest::Client::new());

    let mut body = HashMap::new();
    body.insert("name", "hotpath");

    // Gated behind reqwest-middleware-05/json.
    let resp = client
        .post(format!("http://127.0.0.1:{port}/users"))
        .json(&body)
        .send()
        .await?;
    assert_eq!(resp.status().as_u16(), 200);

    // Gated behind reqwest-middleware-05/query.
    let resp = client
        .get(format!("http://127.0.0.1:{port}/users"))
        .query(&[("page", "1"), ("per_page", "10")])
        .send()
        .await?;
    assert_eq!(resp.status().as_u16(), 200);

    // Gated behind reqwest-middleware-05/form.
    let resp = client
        .post(format!("http://127.0.0.1:{port}/login"))
        .form(&[("user", "bob"), ("pass", "hunter2")])
        .send()
        .await?;
    assert_eq!(resp.status().as_u16(), 200);

    println!("Gated methods example completed");
    Ok(())
}
