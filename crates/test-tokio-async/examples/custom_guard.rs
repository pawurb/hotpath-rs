//! Run with:
//!   cargo run -p test-tokio-async --example custom_guard --features hotpath

use std::time::Duration;

#[hotpath::measure]
fn sync_function(sleep: u64) {
    let vec1 = vec![
        1, 2, 3, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20,
    ];
    std::hint::black_box(&vec1);
    drop(vec1);
    let vec2 = vec![
        1, 2, 3, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20,
    ];
    std::hint::black_box(&vec2);
    std::thread::sleep(Duration::from_nanos(sleep));
}

#[hotpath::measure]
async fn async_function(sleep: u64) {
    let vec1 = vec![1, 2, 3, 5, 6, 7, 8, 9, 10];
    std::hint::black_box(&vec1);
    drop(vec1);
    let vec = vec![1, 2, 3, 5, 6, 7, 8, 9, 10];
    std::hint::black_box(&vec);
    tokio::time::sleep(Duration::from_nanos(sleep)).await;
}

// One call site for the block label: literal `measure_block!` labels must be
// unique per crate, so the workload that runs before and after the guard drop
// is shared instead of duplicated.
async fn workload() {
    for i in 0..50 {
        sync_function(i);
        async_function(i * 2).await;

        hotpath::measure_block!("custom_block", {
            if i == 0 {
                println!("custom_block output");
            }
            std::thread::sleep(Duration::from_nanos(i * 3))
        });
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _hotpath = hotpath::HotpathGuardBuilder::new("custom_guard::main")
        .percentiles(&[50.0, 90.0, 95.0])
        .build();

    workload().await;

    // This will print the report.
    #[allow(clippy::drop_non_drop)]
    drop(_hotpath);

    workload().await;

    // This will cause the program to exit without running destructors,
    // so the report would not be printed for `hotpath::main` macro.
    std::process::exit(1);

    #[allow(unreachable_code)]
    Ok(())
}
