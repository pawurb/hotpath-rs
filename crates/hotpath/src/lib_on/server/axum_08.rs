//! axum 0.8 front-end: tower `Layer`/`Service` impls for [`AxumLayer`] that
//! read the matched route template from `MatchedPath` and time each request
//! until its response head is produced.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{ready, Context, Poll};

use axum::extract::MatchedPath;
use axum::http::{Request, Response};
use pin_project_lite::pin_project;
use tower_layer::Layer;
use tower_service::Service;

use crate::instant::Instant;
use crate::lib_on::caller_stack::{enter_route, intern_route, route_scope_enabled, RequestCalls};
use crate::lib_on::server::{send_server_event, AxumLayer, ServerEvent};

impl<S> Layer<S> for AxumLayer {
    type Service = AxumService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        AxumService { inner }
    }
}

/// Service produced by [`AxumLayer`]; wraps the inner service's future so the
/// completed request is reported when the response head is ready.
#[derive(Clone, Debug)]
pub struct AxumService<S> {
    inner: S,
}

impl<S, ReqBody, ResBody> Service<Request<ReqBody>> for AxumService<S>
where
    S: Service<Request<ReqBody>, Response = Response<ResBody>>,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = AxumResponseFuture<S::Future>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request<ReqBody>) -> Self::Future {
        let (route, matched) = route_key(&req);
        let scope = (matched && route_scope_enabled())
            .then(|| intern_route(&route))
            .flatten();
        AxumResponseFuture {
            inner: self.inner.call(req),
            route,
            matched,
            scope,
            calls: RequestCalls::ZERO,
            start: Instant::now(),
        }
    }
}

/// `METHOD template` when the router matched a route (`MatchedPath` is set by
/// `Router::layer` middleware, including on nested routers), otherwise the raw
/// `METHOD path` flagged as unmatched so the worker normalizes it. Query string
/// and host are dropped by construction.
fn route_key<B>(req: &Request<B>) -> (Arc<str>, bool) {
    let _suspend = crate::lib_on::SuspendAllocTracking::new();
    let method = req.method().as_str();
    match req.extensions().get::<MatchedPath>() {
        Some(path) => (format!("{method} {}", path.as_str()).into(), true),
        None => (format!("{method} {}", req.uri().path()).into(), false),
    }
}

pin_project! {
    /// Response future of [`AxumService`].
    pub struct AxumResponseFuture<F> {
        #[pin]
        inner: F,
        route: Arc<str>,
        matched: bool,
        // Only matched templates are interned: raw paths of unmatched requests
        // are unbounded and would leak through the route interner.
        scope: Option<&'static str>,
        // SQL queries / outbound HTTP requests issued so far under `scope`.
        calls: RequestCalls,
        start: Instant,
    }
}

impl<F, ResBody, E> Future for AxumResponseFuture<F>
where
    F: Future<Output = Result<Response<ResBody>, E>>,
{
    type Output = F::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();
        let outcome = {
            // The guard writes the scope's call counts back into `this.calls`
            // on drop, including on the early return of `ready!`.
            let _route_scope = this.scope.map(|route| enter_route(route, this.calls));
            ready!(this.inner.poll(cx))
        };
        if let Ok(response) = &outcome {
            send_server_event(ServerEvent::Completed {
                route: Arc::clone(this.route),
                matched: *this.matched,
                duration_nanos: this.start.elapsed().as_nanos() as u64,
                status: response.status().as_u16(),
                timestamp_ns: crate::lib_on::current_elapsed_ns(),
                calls: this.scope.map(|_| *this.calls),
            });
        }
        Poll::Ready(outcome)
    }
}
