//! Demonstrates `hotpath::http!` instrumenting a ureq 3 agent config once at
//! creation. Every request sent through the built agent is timed and bucketed
//! by normalized endpoint (`GET 127.0.0.1:{port}/users/{id}`), with transport
//! errors and 4xx/5xx responses counted in the `Errors` column. Each request
//! originates from an instrumented `Data` method, so the report's `Source`
//! column attributes it to that method.
//!
//! Run with:
//!   cargo run -p test-ureq --example basic --features hotpath

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

struct Data {
    agent: ureq::Agent,
    ext_agent: ureq::Agent,
    port: u16,
}

#[hotpath::measure_all]
impl Data {
    fn new(port: u16) -> Self {
        // The middleware is appended to the config builder; the built agent
        // is a plain ureq::Agent whether `hotpath` is enabled or not.
        let agent: ureq::Agent = hotpath::http!(ureq::Agent::config_builder())
            .build()
            .new_agent();

        // Labeled agent: keys are prefixed, so its requests land in their own bucket.
        let ext_agent: ureq::Agent = hotpath::http!(ureq::Agent::config_builder(), label = "ext")
            .build()
            .new_agent();

        Self {
            agent,
            ext_agent,
            port,
        }
    }

    fn fetch_user(&self, id: u32) -> Result<(), Box<dyn std::error::Error>> {
        let resp = self
            .agent
            .get(format!(
                "http://127.0.0.1:{}/users/{id}?verbose=true",
                self.port
            ))
            .call()?;
        assert_eq!(resp.status().as_u16(), 200);
        Ok(())
    }

    fn fetch_stats(&self) {
        // No /stats route on the server - ureq surfaces the 404 as
        // `Error::StatusCode`, which counts as an error with status 404.
        let result = self
            .agent
            .get(format!("http://127.0.0.1:{}/stats", self.port))
            .call();
        assert!(matches!(result, Err(ureq::Error::StatusCode(404))));
    }

    fn ping_replica(&self, replica_port: u16) {
        // Connection refused (nothing listens on the dropped port) counts as an error.
        let result = self
            .agent
            .get(format!("http://127.0.0.1:{replica_port}/health"))
            .call();
        assert!(result.is_err());
    }

    fn fetch_external_user(&self, id: u32) -> Result<(), Box<dyn std::error::Error>> {
        let resp = self
            .ext_agent
            .get(format!("http://127.0.0.1:{}/users/{id}", self.port))
            .call()?;
        assert_eq!(resp.status().as_u16(), 200);
        Ok(())
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _guard = HotpathGuardBuilder::new("main")
        .sections(vec![Section::Http])
        .build();

    let port = start_server();
    let data = Data::new(port);

    // Two ids, one bucket: GET 127.0.0.1:{port}/users/{id}.
    for id in 1..=2 {
        data.fetch_user(id)?;
    }

    data.fetch_stats();

    let replica_port = {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        listener.local_addr()?.port()
    };
    data.ping_replica(replica_port);

    data.fetch_external_user(3)?;

    println!("HTTP example completed");
    Ok(())
}
