use axum::{Router, middleware::from_fn};
use hotpath_backend::config::{middleware, routes::app};
use reqwest::StatusCode;
use std::time::Duration;
use tower_http::{
    catch_panic::CatchPanicLayer,
    compression::{
        CompressionLayer,
        predicate::{DefaultPredicate, NotForContentType, Predicate},
    },
    timeout::TimeoutLayer,
};

fn build_app() -> Router {
    app()
        .layer(from_fn(middleware::request_tracing))
        .layer(TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            Duration::from_secs(10),
        ))
        .layer(
            CompressionLayer::new()
                .compress_when(DefaultPredicate::new().and(NotForContentType::new("video/"))),
        )
        .layer(CatchPanicLayer::new())
        .layer(from_fn(middleware::security_headers))
}

#[tokio::main(flavor = "current_thread")]
#[hotpath::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let app = build_app();
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3001").await.unwrap();
    println!("Server listening on http://0.0.0.0:3001");
    axum::serve(listener, app).await.unwrap();
}
