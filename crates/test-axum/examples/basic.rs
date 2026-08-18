//! Demonstrates `hotpath::axum!` wrapping an axum 0.8 router once at
//! creation. Every request the router serves is timed until its response head
//! is produced and bucketed by matched route template (`GET /users/{id}`), so
//! parameter-varied requests to the same route merge into one row. 4xx and
//! 5xx responses are counted in their own columns; requests that match no
//! route (the fallback) are bucketed by their normalized raw path.
//!
//! Run with:
//!   cargo run -p test-axum --example basic --features hotpath

use axum::extract::Path;
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::Router;
use hotpath::{HotpathGuardBuilder, Section};
use std::time::Duration;

async fn get_user(Path(id): Path<u32>) -> Result<String, StatusCode> {
    tokio::time::sleep(Duration::from_millis(5)).await;
    if id == 0 {
        return Err(StatusCode::NOT_FOUND);
    }
    Ok(format!("user {id}"))
}

async fn create_user() -> StatusCode {
    tokio::time::sleep(Duration::from_millis(10)).await;
    StatusCode::CREATED
}

async fn crash() -> StatusCode {
    StatusCode::INTERNAL_SERVER_ERROR
}

async fn not_found() -> StatusCode {
    StatusCode::NOT_FOUND
}

fn app() -> Router {
    // The macro must wrap the finished router: `Router::layer` only applies
    // to routes that were added before it.
    hotpath::axum!(Router::new()
        .route("/users/{id}", get(get_user))
        .route("/users", post(create_user))
        .route("/crash", get(crash))
        .fallback(not_found))
}

fn send_requests(port: u16) {
    let agent = ureq::Agent::new_with_defaults();
    let base = format!("http://127.0.0.1:{port}");

    // Two ids, one bucket: GET /users/{id}.
    for id in 1..=2 {
        let resp = agent
            .get(format!("{base}/users/{id}?verbose=true"))
            .call()
            .expect("GET /users/{id}");
        assert_eq!(resp.status().as_u16(), 200);
    }

    let err = agent.get(format!("{base}/users/0")).call().unwrap_err();
    assert!(matches!(err, ureq::Error::StatusCode(404)));

    let resp = agent
        .post(format!("{base}/users"))
        .send_empty()
        .expect("POST /users");
    assert_eq!(resp.status().as_u16(), 201);

    let err = agent.get(format!("{base}/crash")).call().unwrap_err();
    assert!(matches!(err, ureq::Error::StatusCode(500)));

    // No matching route: bucketed by normalized raw path (GET /missing/{id}).
    let err = agent.get(format!("{base}/missing/42")).call().unwrap_err();
    assert!(matches!(err, ureq::Error::StatusCode(404)));
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _guard = HotpathGuardBuilder::new("main")
        .sections(vec![Section::Server])
        .build();

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();
    tokio::spawn(async move {
        axum::serve(listener, app()).await.expect("axum server");
    });

    tokio::task::spawn_blocking(move || send_requests(port)).await?;

    if let Ok(secs) = std::env::var("TEST_SLEEP_SECONDS") {
        if let Ok(secs) = secs.parse::<u64>() {
            tokio::time::sleep(Duration::from_secs(secs)).await;
        }
    }

    println!("axum example completed");
    Ok(())
}
