//! Run with:
//!   cargo run -p test-tokio-async --example guard_timeout_functions --features hotpath

use std::time::Duration;

#[hotpath::measure]
fn looping_function() {
    std::hint::black_box(42);
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    hotpath::HotpathGuardBuilder::new("guard_timeout_functions")
        .build_with_shutdown(Duration::from_secs(1));

    loop {
        looping_function();
    }
}
