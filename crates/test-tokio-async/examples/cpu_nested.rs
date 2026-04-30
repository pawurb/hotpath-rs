//! CPU-bound profiling example with nested measured functions.
//!
//! `outer_work` calls `heavy_work` and `light_work`. Used to validate
//! exclusive vs inclusive CPU attribution: under exclusive, samples inside
//! `heavy_work`/`light_work` should attribute to them, not `outer_work`.
//! Under inclusive, `outer_work` should also accumulate samples from its
//! callees.
//!
//! Profile with hotpath:
//! ```bash
//! cargo run --example cpu_nested --features hotpath --profile profiling
//! ```
//!
//! Profile with samply:
//! ```bash
//! cargo build --example cpu_nested --profile profiling
//! samply record ./target/profiling/examples/cpu_nested
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

#[hotpath::measure]
fn outer_work() -> u64 {
    let mut total: u64 = 0;
    total = total.wrapping_add(heavy_work(500_000));
    total = total.wrapping_add(light_work(100_000));
    total
}

#[hotpath::main]
fn main() {
    let mut total: u64 = 0;
    for _ in 0..1000 {
        total = total.wrapping_add(outer_work());
    }
    black_box(total);
}
