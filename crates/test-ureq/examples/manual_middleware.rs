//! Attaches `hotpath::UreqHttpMiddleware` to a ureq config by hand instead of
//! via the `http!` macro, which lets an app control where hotpath sits in its
//! middleware chain. Here a request-counting middleware wraps hotpath's, so
//! the counter runs outside the timed window. The `.middleware(...)` line
//! compiles unchanged with `hotpath` disabled thanks to the no-op middleware.
//!
//! Run with:
//!   cargo run -p test-ureq --example manual_middleware --features hotpath

use hotpath::{HotpathGuardBuilder, Section};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

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

struct CountingMiddleware(Arc<AtomicUsize>);

impl ureq::middleware::Middleware for CountingMiddleware {
    fn handle(
        &self,
        req: ureq::http::Request<ureq::SendBody>,
        next: ureq::middleware::MiddlewareNext,
    ) -> Result<ureq::http::Response<ureq::Body>, ureq::Error> {
        self.0.fetch_add(1, Ordering::Relaxed);
        next.handle(req)
    }
}

#[hotpath::measure]
fn fetch_users(agent: &ureq::Agent, port: u16) -> Result<(), Box<dyn std::error::Error>> {
    for id in 1..=3 {
        agent
            .get(format!("http://127.0.0.1:{port}/users/{id}"))
            .call()?;
    }
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _guard = HotpathGuardBuilder::new("main")
        .sections(vec![Section::Http])
        .build();

    let port = start_server();
    let counter = Arc::new(AtomicUsize::new(0));

    let agent: ureq::Agent = ureq::Agent::config_builder()
        .middleware(CountingMiddleware(Arc::clone(&counter)))
        .middleware(hotpath::UreqHttpMiddleware::with_label("manual"))
        .build()
        .new_agent();

    fetch_users(&agent, port)?;
    assert_eq!(counter.load(Ordering::Relaxed), 3);

    println!("HTTP manual middleware example completed");
    Ok(())
}
