//! reqwest 0.13 front-end: `Middleware` impl for reqwest-middleware 0.5 and
//! the `InstrumentHttpClient` routing for `reqwest::Client`.

use ::http::Extensions;
use async_trait::async_trait;
use reqwest_middleware_05::{ClientBuilder, ClientWithMiddleware, Error, Middleware, Next, Result};

use crate::instant::Instant;
use crate::lib_on::http::{
    endpoint_pre_key, send_http_event, HttpEvent, InstrumentHttpClient, ReqwestHttpMiddleware,
};

#[async_trait]
impl Middleware for ReqwestHttpMiddleware {
    async fn handle(
        &self,
        req: reqwest::Request,
        extensions: &mut Extensions,
        next: Next<'_>,
    ) -> Result<reqwest::Response> {
        let url = req.url();
        let endpoint = endpoint_pre_key(
            req.method().as_str(),
            url.host_str(),
            url.port(),
            url.path(),
        );

        let start = Instant::now();
        let outcome = next.run(req, extensions).await;
        let duration_nanos = start.elapsed().as_nanos() as u64;

        let status = match &outcome {
            Ok(resp) => Some(resp.status().as_u16()),
            Err(Error::Reqwest(e)) => e.status().map(|s| s.as_u16()),
            Err(_) => None,
        };

        send_http_event(HttpEvent::Executed {
            endpoint: endpoint.into(),
            label: self.label.as_deref().map(Into::into),
            duration_nanos,
            status,
            timestamp_ns: crate::lib_on::current_elapsed_ns(),
        });

        outcome
    }
}

impl InstrumentHttpClient for reqwest::Client {
    type Output = ClientWithMiddleware;

    fn instrument_http(self, label: Option<String>) -> Self::Output {
        ClientBuilder::new(self)
            .with(ReqwestHttpMiddleware { label })
            .build()
    }
}
