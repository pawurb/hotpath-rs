#[cfg_attr(feature = "hotpath", hotpath::measure_all)]
mod measured_module {
    use std::time::Duration;

    pub fn sync_function_one(sleep: u64) {
        let vec = vec![1, 2, 3, 4, 5];
        std::hint::black_box(&vec);
        drop(vec);
        std::thread::sleep(Duration::from_nanos(sleep));
    }

    #[cfg_attr(feature = "hotpath", hotpath::skip)]
    pub fn sync_function_two(sleep: u64) {
        let vec = vec![6, 7, 8, 9, 10];
        std::hint::black_box(&vec);
        drop(vec);
        std::thread::sleep(Duration::from_nanos(sleep * 2));
    }

    pub async fn async_function_one(sleep: u64) {
        let vec = vec![1, 2, 3];
        std::hint::black_box(&vec);
        drop(vec);
        tokio::time::sleep(Duration::from_nanos(sleep)).await;
    }

    #[cfg_attr(feature = "hotpath", hotpath::skip)]
    pub async fn async_function_two(sleep: u64) {
        let vec = vec![4, 5, 6];
        std::hint::black_box(&vec);
        drop(vec);
        tokio::time::sleep(Duration::from_nanos(sleep * 2)).await;
    }
}

#[tokio::main(flavor = "current_thread")]
#[cfg_attr(feature = "hotpath", hotpath::main)]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    for i in 1..=50 {
        measured_module::sync_function_one(i);
        measured_module::sync_function_two(i);
        measured_module::async_function_one(i * 2).await;
        measured_module::async_function_two(i * 2).await;
    }

    Ok(())
}
