#[cfg_attr(feature = "hotpath", hotpath::measure)]
fn noop_sync_function() {
    let vec1 = vec![1, 2, 3, 5, 6, 7, 8, 9, 10];
    let vec2 = vec![1, 2, 3, 5, 6, 7, 8, 9, 10];
    std::hint::black_box((vec1, vec2));
}

#[cfg_attr(feature = "hotpath", hotpath::measure)]
async fn noop_async_function() {
    let vec1 = vec![1, 2, 3, 5, 6, 10];
    std::hint::black_box(vec1);
}

#[tokio::main(flavor = "current_thread")]
#[cfg_attr(feature = "hotpath", hotpath::main)]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    for _ in 0..100_000 {
        noop_sync_function();
        noop_async_function().await;
    }

    Ok(())
}
