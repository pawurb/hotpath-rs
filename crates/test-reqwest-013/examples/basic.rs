//! Demonstrates `hotpath::http!` wrapping a reqwest 0.13 client once at
//! creation. Every request sent through the returned client is timed and
//! bucketed by normalized endpoint (`GET 127.0.0.1:{port}/users/{id}`), with
//! transport errors and 4xx/5xx responses counted in the `Errors` column.
//!
//! Run with:
//!   cargo run -p test-reqwest-013 --example basic --features hotpath

use hotpath::{HotpathGuardBuilder, Section};
use std::net::TcpListener;

fn start_server() -> u16 {
    let server = tiny_http::Server::http("127.0.0.1:0").expect("bind test server");
    let port = server.server_addr().to_ip().expect("ip listener").port();
    std::thread::spawn(move || {
        for request in server.incoming_requests() {
            let (body, status) = if request.url().starts_with("/users/") {
                ("{\"ok\":true}", 200)
            } else {
                ("not found", 404)
            };
            let response = tiny_http::Response::from_string(body).with_status_code(status);
            let _ = request.respond(response);
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

    // The annotation pins the wrap alias: ClientWithMiddleware with `hotpath`
    // enabled, raw reqwest::Client otherwise - same written type either way.
    let client: hotpath::wrap::reqwest::Client = hotpath::http!(reqwest::Client::new());

    // Two ids, one bucket: GET 127.0.0.1:{port}/users/{id}.
    for id in 1..=2 {
        let resp = client
            .get(format!("http://127.0.0.1:{port}/users/{id}?verbose=true"))
            .send()
            .await?;
        assert_eq!(resp.status().as_u16(), 200);
    }

    // 404 counts as an error.
    let resp = client
        .get(format!("http://127.0.0.1:{port}/missing"))
        .send()
        .await?;
    assert_eq!(resp.status().as_u16(), 404);

    // Connection refused (nothing listens on the dropped port) counts as an error.
    let refused_port = {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        listener.local_addr()?.port()
    };
    let result = client
        .get(format!("http://127.0.0.1:{refused_port}/unreachable"))
        .send()
        .await;
    assert!(result.is_err());

    // Labeled client: keys are prefixed, so this lands in its own bucket.
    let labeled: hotpath::wrap::reqwest::Client =
        hotpath::http!(reqwest::Client::new(), label = "ext");
    let resp = labeled
        .get(format!("http://127.0.0.1:{port}/users/3"))
        .send()
        .await?;
    assert_eq!(resp.status().as_u16(), 200);

    println!("HTTP example completed");
    Ok(())
}
