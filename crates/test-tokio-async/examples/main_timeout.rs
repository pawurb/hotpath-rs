use std::time::Duration;

#[cfg_attr(feature = "hotpath", hotpath::measure)]
fn first_function(sleep: u64) {
    let vec1 = vec![
        1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20,
    ];
    std::hint::black_box(&vec1);
    std::thread::sleep(Duration::from_micros(sleep));
}

#[cfg_attr(feature = "hotpath", hotpath::measure)]
fn second_function(sleep: u64) {
    let vec1 = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
    std::hint::black_box(&vec1);
    std::thread::sleep(Duration::from_micros(sleep));
}

#[tokio::main(flavor = "current_thread")]
#[cfg_attr(feature = "hotpath", hotpath::main(timeout = 1000))]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    loop {
        first_function(100);
        second_function(50);

        #[cfg(feature = "hotpath")]
        hotpath::measure_block!("loop_block", {
            let vec = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
            std::hint::black_box(&vec);
            std::thread::sleep(Duration::from_micros(100));
        });
    }

    #[allow(unreachable_code)]
    Ok(())
}
