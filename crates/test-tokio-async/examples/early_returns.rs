//! Run with:
//!   cargo run -p test-tokio-async --example early_returns --features hotpath

use std::time::Duration;

#[hotpath::measure]
fn early_return() {
    // Work before returning…
    std::thread::sleep(Duration::from_millis(10));

    if true {
        return;
    }

    unreachable!();
}

fn may_fail(flag: bool) -> Result<(), &'static str> {
    std::thread::sleep(Duration::from_millis(5));
    if flag {
        Err("boom")
    } else {
        Ok(())
    }
}

#[hotpath::measure]
fn propagates_error() -> Result<(), &'static str> {
    may_fail(true)?;
    unreachable!();
}

#[hotpath::measure]
fn normal_path() {
    std::thread::sleep(Duration::from_millis(15));
}

#[hotpath::main]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    early_return();
    let _ = propagates_error();
    normal_path();

    Ok(())
}
