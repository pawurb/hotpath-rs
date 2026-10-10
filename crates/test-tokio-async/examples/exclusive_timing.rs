//! Nested and recursive sync functions with known own/nested time splits,
//! for comparing inclusive and exclusive (`HOTPATH_TIME_EXCLUSIVE`) function
//! timing.
//!
//! Run with:
//!   HOTPATH_TIME_EXCLUSIVE=true cargo run -p test-tokio-async --example exclusive_timing --features hotpath

use std::time::{Duration, Instant};

/// Spins instead of sleeping: sleeps overshoot by tens of milliseconds on
/// loaded CI runners, which would blur the own/nested time splits.
fn busy(ms: u64) {
    let start = Instant::now();
    while start.elapsed() < Duration::from_millis(ms) {
        std::hint::spin_loop();
    }
}

#[hotpath::measure]
fn sync_child() {
    busy(100);
}

#[hotpath::measure]
fn sync_parent() {
    busy(50);
    sync_child();
}

#[hotpath::measure]
fn recursive(depth: u32) {
    busy(20);
    if depth > 1 {
        recursive(depth - 1);
    }
}

#[hotpath::measure]
fn focus_leaf() {
    busy(60);
}

#[hotpath::measure]
fn focus_mid() {
    busy(30);
    focus_leaf();
}

#[hotpath::measure]
fn focus_outer() {
    busy(30);
    focus_mid();
}

#[hotpath::main(format = "json")]
fn main() {
    sync_parent();
    recursive(4);
    focus_outer();
}
