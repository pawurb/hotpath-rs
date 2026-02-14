use std::time::Instant;

use hotpath::{format_debug_truncated, MAX_RESULT_LEN};

const ITERATIONS: usize = 1000;

#[derive(Debug)]
#[allow(dead_code)]
struct LargePayload {
    id: u64,
    name: String,
    tags: Vec<String>,
    data: Vec<u8>,
}

fn make_payload(i: u64, data_size: usize) -> LargePayload {
    LargePayload {
        id: i,
        name: format!("item-{}", i),
        tags: (0..20).map(|t| format!("tag-{}-{}", i, t)).collect(),
        data: vec![0xAB; data_size],
    }
}

#[hotpath::measure]
fn format_then_truncate(value: &impl std::fmt::Debug) -> String {
    let s = format!("{:?}", value);
    if s.len() <= MAX_RESULT_LEN {
        s
    } else {
        let end = hotpath::floor_char_boundary(&s, MAX_RESULT_LEN.saturating_sub(3));
        format!("{}...", &s[..end])
    }
}

#[hotpath::measure]
fn bench_format_debug_truncated(value: &impl std::fmt::Debug) -> String {
    format_debug_truncated(value)
}

#[hotpath::main]
fn main() {
    let payload_size: usize = std::env::var("PAYLOAD_SIZE")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(4096);
    let payloads: Vec<LargePayload> = (0..ITERATIONS as u64)
        .map(|i| make_payload(i, payload_size))
        .collect();

    // Warmup
    for p in &payloads {
        std::hint::black_box(format_then_truncate(p));
        std::hint::black_box(bench_format_debug_truncated(p));
    }

    let start = Instant::now();
    let mut total_bytes_old = 0usize;
    for p in &payloads {
        let s = format_then_truncate(p);
        total_bytes_old += s.len();
        std::hint::black_box(s);
    }
    let old_elapsed = start.elapsed();

    let start = Instant::now();
    let mut total_bytes_new = 0usize;
    for p in &payloads {
        let s = bench_format_debug_truncated(p);
        total_bytes_new += s.len();
        std::hint::black_box(s);
    }
    let new_elapsed = start.elapsed();

    println!(
        "Payload Debug size: ~{} bytes",
        format!("{:?}", payloads[0]).len()
    );
    println!("Iterations: {}", ITERATIONS);
    println!();
    println!(
        "format!() + truncate:      {:>10.2?}  ({} total bytes)",
        old_elapsed, total_bytes_old
    );
    println!(
        "format_debug_truncated:    {:>10.2?}  ({} total bytes)",
        new_elapsed, total_bytes_new
    );
    println!();

    let speedup = old_elapsed.as_nanos() as f64 / new_elapsed.as_nanos() as f64;
    println!("Speedup: {:.2}x", speedup);
}
