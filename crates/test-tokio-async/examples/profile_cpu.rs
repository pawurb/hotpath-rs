//! CPU-bound profiling example
//!
//! Compares hotpath instrumentation vs sampling profilers.
//!
//! Profile with hotpath:
//! ```bash
//! cargo run --example profile_cpu --features hotpath --profile profiling
//! ```
//!
//! Profile with samply:
//! ```bash
//! cargo build --example profile_cpu --profile profiling
//! samply record ./target/profiling/examples/profile_cpu
//! ```

use std::hint::black_box;

#[hotpath::measure]
fn heavy_work(iterations: u32) -> u64 {
    let mut result: u64 = 1;
    for i in 0..iterations {
        result = result.wrapping_mul(black_box(i as u64).wrapping_add(7));
        result ^= result >> 3;
    }
    result
}

#[hotpath::measure]
fn light_work(iterations: u32) -> u64 {
    let mut result: u64 = 0;
    for i in 0..iterations {
        result = result.wrapping_add(black_box(i as u64));
    }
    result
}

#[hotpath::main]
fn main() {
    let mut total: u64 = 0;

    for _ in 0..1000 {
        total = total.wrapping_add(heavy_work(500_000));
        total = total.wrapping_add(light_work(100_000));
    }
}
