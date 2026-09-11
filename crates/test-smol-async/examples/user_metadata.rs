//! Run with:
//!   cargo run -p test-smol-async --example user_metadata --features hotpath
//!
//! Builds the guard with builder-supplied user metadata so the integration
//! test can check how it merges with `HOTPATH_USER_METADATA`.

use std::collections::HashMap;
use std::time::Duration;

#[hotpath::measure]
async fn work(input: u64) -> u64 {
    smol::Timer::after(Duration::from_millis(1)).await;
    std::hint::black_box(input + 1)
}

fn main() {
    let metadata = HashMap::from([
        ("source".to_string(), "builder".to_string()),
        ("team".to_string(), "core".to_string()),
    ]);
    let _guard = hotpath::HotpathGuardBuilder::new("main")
        .sections(vec![hotpath::Section::FunctionsTiming])
        .user_metadata(metadata)
        .build();

    smol::block_on(async {
        let mut total = 0_u64;
        for i in 0..3_u64 {
            total += work(i).await;
        }
        std::hint::black_box(total);
    });
}
