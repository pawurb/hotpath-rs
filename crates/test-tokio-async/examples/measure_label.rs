use std::time::Duration;

#[hotpath::measure(label = "sync_labeled")]
fn sync_function() {
    std::thread::sleep(Duration::from_micros(10));
}

#[hotpath::measure(label = "async_labeled", log = true)]
async fn async_function() -> u64 {
    tokio::time::sleep(Duration::from_micros(10)).await;
    42
}

#[tokio::main(flavor = "current_thread")]
#[hotpath::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    for _ in 0..10 {
        sync_function();
        async_function().await;
    }
    Ok(())
}
