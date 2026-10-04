//! Tiny benchmark uploaded to many series at once by
//! `.github/workflows/cloud-limit-benchmark.yml` to exercise the per-repo
//! benchmark limit.
//!
//! Run with:
//!   cargo run --release -p test-alloc --example benchmark_limit --features hotpath,hotpath-alloc

#[hotpath::measure]
fn build(i: usize) -> String {
    format!("item-{i}")
}

#[hotpath::main]
fn main() {
    let total: usize = (0..1_000).map(|i| build(i).len()).sum();
    println!("built {total} bytes");
}
