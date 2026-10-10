use std::net::TcpStream;
use std::time::Duration;

/// A local service some tests depend on (see `docker-compose.yml.sample`).
pub(crate) struct Service {
    name: &'static str,
    port: u16,
    compose_service: &'static str,
}

pub(crate) const POSTGRES: Service = Service {
    name: "PostgreSQL",
    port: 5439,
    compose_service: "postgres",
};

pub(crate) const REDIS: Service = Service {
    name: "Redis",
    port: 6390,
    compose_service: "redis",
};

impl Service {
    fn available(&self) -> bool {
        let addr = ([127, 0, 0, 1], self.port).into();
        TcpStream::connect_timeout(&addr, Duration::from_millis(500)).is_ok()
    }

    /// True when `test` must return early because nothing listens on the
    /// service port. On CI, where the service is mandatory, panics instead.
    #[track_caller]
    pub(crate) fn skip_if_unavailable(&self, test: &str) -> bool {
        if self.available() {
            return false;
        }
        let Self {
            name,
            port,
            compose_service,
        } = self;
        assert!(
            std::env::var_os("CI").is_none(),
            "no {name} on localhost:{port} - the CI {compose_service} service is required"
        );
        eprintln!(
            "skipping {test}: no {name} on localhost:{port} \
             (start it with `docker compose up -d {compose_service}`)"
        );
        true
    }
}
