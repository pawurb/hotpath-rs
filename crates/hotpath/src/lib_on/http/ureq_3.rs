//! ureq 3 front-end: `Middleware` impl for ureq's blocking agent and the
//! `InstrumentHttpClient` routing for `ConfigBuilder<AgentScope>`.

use ureq::config::ConfigBuilder;
use ureq::http::{Request, Response, Uri};
use ureq::middleware::{Middleware, MiddlewareNext};
use ureq::typestate::AgentScope;
use ureq::{Body, Error, SendBody};

use crate::instant::Instant;
use crate::lib_on::http::{
    endpoint_pre_key, send_http_event, HttpEvent, InstrumentHttpClient, UreqHttpMiddleware,
};

impl Middleware for UreqHttpMiddleware {
    fn handle(
        &self,
        req: Request<SendBody>,
        next: MiddlewareNext,
    ) -> Result<Response<Body>, Error> {
        let uri = req.uri();
        let endpoint = endpoint_pre_key(
            req.method().as_str(),
            uri.host(),
            explicit_port(uri),
            uri.path(),
        );

        let source = crate::lib_on::caller_stack::current_caller();
        let route = crate::lib_on::caller_stack::current_http_route();
        let start = Instant::now();
        let outcome = next.handle(req);
        let duration_nanos = start.elapsed().as_nanos() as u64;

        // ureq's default `http_status_as_error(true)` surfaces 4xx/5xx as
        // `Error::StatusCode`, so the status must be recovered from the error.
        let status = match &outcome {
            Ok(resp) => Some(resp.status().as_u16()),
            Err(Error::StatusCode(code)) => Some(*code),
            Err(_) => None,
        };

        send_http_event(HttpEvent::Executed {
            endpoint: endpoint.into(),
            label: self.label.as_deref().map(Into::into),
            duration_nanos,
            status,
            timestamp_ns: crate::lib_on::current_elapsed_ns(),
            source,
            route,
        });

        outcome
    }
}

/// `Uri::port_u16` keeps an explicitly written scheme-default port
/// (`https://host:443/`); drop it so the request buckets with the bare form.
fn explicit_port(uri: &Uri) -> Option<u16> {
    let port = uri.port_u16()?;
    let default = match uri.scheme_str() {
        Some("http") => 80,
        Some("https") => 443,
        _ => return Some(port),
    };
    (port != default).then_some(port)
}

impl InstrumentHttpClient for ConfigBuilder<AgentScope> {
    type Output = Self;

    fn instrument_http(self, label: Option<String>) -> Self::Output {
        self.middleware(UreqHttpMiddleware { label })
    }
}
