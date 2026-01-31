use hotpath::future;
use hotpath::futures::FuturesGuardBuilder;
use hotpath::Format;
use std::time::Duration;

async fn slow_operation() -> i32 {
    tokio::time::sleep(Duration::from_millis(10)).await;
    42
}

#[tokio::main]
async fn main() {
    let _guard = FuturesGuardBuilder::new()
        .format(Format::Json)
        .output_path("tmp/futures_output_test.json")
        .build();

    let result = future!(slow_operation()).await;
    println!("Result: {}", result);

    let result = future!(slow_operation()).await;
    println!("Result: {}", result);
}
