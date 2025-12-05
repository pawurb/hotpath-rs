use std::time::Duration;

#[hotpath::measure]
fn first_function(sleep: u64) {
    let vec1 = vec![
        1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20,
    ];
    std::hint::black_box(&vec1);
    std::thread::sleep(Duration::from_micros(sleep));
}

#[hotpath::measure]
fn second_function(sleep: u64) {
    let vec1 = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
    std::hint::black_box(&vec1);
    std::thread::sleep(Duration::from_micros(sleep));
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    hotpath::FunctionsGuardBuilder::new("guard_timeout::main")
        .build_with_timeout(Duration::from_secs(1));

    loop {
        first_function(100);
        second_function(50);

        hotpath::measure_block!("loop_block", {
            let vec = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
            std::hint::black_box(&vec);
            std::thread::sleep(Duration::from_micros(100));
        });
    }

    #[allow(unreachable_code)]
    Ok(())
}
