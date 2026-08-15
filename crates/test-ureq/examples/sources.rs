//! Demonstrates HTTP source attribution: the same endpoint requested from two
//! instrumented functions lands in two report entries, each tagged with the
//! function it was issued from. Requests sent outside any measured scope get
//! no source.
//!
//! Run with:
//!   cargo run -p test-ureq --example sources --features hotpath

use hotpath::{HotpathGuardBuilder, Section};

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
fn fetch_from_a(agent: &ureq::Agent, port: u16) -> Result<(), Box<dyn std::error::Error>> {
    agent
        .get(format!("http://127.0.0.1:{port}/users/1"))
        .call()?;
    Ok(())
}

#[hotpath::measure]
fn fetch_from_b(agent: &ureq::Agent, port: u16) -> Result<(), Box<dyn std::error::Error>> {
    for id in 2..=3 {
        agent
            .get(format!("http://127.0.0.1:{port}/users/{id}"))
            .call()?;
    }
    Ok(())
}

// Attribution picks the innermost instrumented function: requests issued by
// `fetch_from_a` stay attributed to it even when it runs inside another
// measured function.
#[hotpath::measure]
fn outer(agent: &ureq::Agent, port: u16) -> Result<(), Box<dyn std::error::Error>> {
    fetch_from_a(agent, port)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _guard = HotpathGuardBuilder::new("main")
        .sections(vec![Section::Http])
        .build();

    let port = start_server();
    let agent: ureq::Agent = hotpath::http!(ureq::Agent::config_builder())
        .build()
        .new_agent();

    fetch_from_a(&agent, port)?;
    fetch_from_b(&agent, port)?;
    outer(&agent, port)?;

    // Same endpoint outside any measured scope: separate, source-less entry.
    agent
        .get(format!("http://127.0.0.1:{port}/users/9"))
        .call()?;

    println!("HTTP sources example completed");
    Ok(())
}
