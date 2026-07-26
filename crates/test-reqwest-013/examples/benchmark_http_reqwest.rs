use hotpath::{HotpathGuardBuilder, Section};
use std::time::{Duration, Instant};

// The annotation pins the wrap alias: ClientWithMiddleware with `hotpath`
// enabled, raw reqwest::Client otherwise - same written type either way.
type Client = hotpath::wrap::reqwest::Client;

// Single-threaded stress test comparing HTTP client instrumentation overhead in one run:
// an uninstrumented baseline (raw reqwest client) and the `hotpath::http!` wrapped
// version, each hammering the same endpoint on a local tiny_http server over a kept-alive
// loopback connection. The delta vs baseline isolates the per-request cost of the
// middleware hop, endpoint normalization, and event enqueue. Run with
// `--features hotpath` (without it the macro is a no-op and both modes are the raw
// client). The full loopback round trip dominates each op, so deltas below ~1µs are
// within run-to-run noise. Iteration count via `HOTPATH_BENCH_RUNS`.

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

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _guard = HotpathGuardBuilder::new("main")
        .sections(vec![Section::Http])
        .build();

    let runs = bench_runs();
    let port = start_server();
    let url = format!("http://127.0.0.1:{port}/users/1");

    // Warm each client's connection pool so neither phase pays one-time TCP
    // setup costs.
    let raw = reqwest::Client::new();
    phase_baseline(&raw, &url, runs / 10).await?;
    let start = Instant::now();
    phase_baseline(&raw, &url, runs).await?;
    let baseline = start.elapsed();

    let client: Client = hotpath::http!(reqwest::Client::new());
    phase_instrumented(&client, &url, runs / 10).await?;
    let start = Instant::now();
    phase_instrumented(&client, &url, runs).await?;
    let instrumented = start.elapsed();

    report("reqwest (local server)", runs, baseline, instrumented);
    Ok(())
}

async fn phase_baseline(
    client: &reqwest::Client,
    url: &str,
    runs: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    for _ in 0..runs {
        let resp = client.get(url).send().await?;
        assert_eq!(resp.status().as_u16(), 200);
    }
    Ok(())
}

// Instrumented so the requests are attributed to this function in the report's
// Source column.
#[hotpath::measure]
async fn phase_instrumented(
    client: &Client,
    url: &str,
    runs: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    for _ in 0..runs {
        let resp = client.get(url).send().await?;
        assert_eq!(resp.status().as_u16(), 200);
    }
    Ok(())
}

fn report(name: &str, runs: u64, baseline: Duration, instrumented: Duration) {
    let per = |d: Duration| d.as_nanos() as f64 / runs as f64;
    let b = per(baseline);
    let ins = per(instrumented);
    println!("\n{name}: {runs} requests per mode");
    println!("  baseline (raw)  {b:>8.1} ns/op");
    println!(
        "  instrumented    {ins:>8.1} ns/op  ({:+.1} ns/op vs baseline)",
        ins - b
    );
}

fn bench_runs() -> u64 {
    std::env::var("HOTPATH_BENCH_RUNS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(10_000)
}
