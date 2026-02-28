use std::hint::black_box;

async fn uninstrumented_1kb() -> Vec<u8> {
    let buf = vec![0u8; 1024];
    black_box(&buf);
    tokio::task::yield_now().await;
    buf
}

#[hotpath::measure]
async fn instrumented_1kb() {
    let buf = vec![0u8; 1024];
    black_box(&buf);
    tokio::task::yield_now().await;
}

#[hotpath::measure]
async fn uninstrumented_children_2kb() {
    let a = uninstrumented_1kb().await;
    let b = uninstrumented_1kb().await;
    black_box((&a, &b));
}

#[hotpath::measure(future = true)]
async fn own_1kb_plus_uninstrumented_child_1kb() {
    let own = vec![0u8; 1024];
    black_box(&own);
    let child = uninstrumented_1kb().await;
    black_box(&child);
}

#[hotpath::measure(future = true)]
async fn own_1kb_plus_uninstrumented_1kb_plus_instrumented_1kb() {
    let own = vec![0u8; 1024];
    black_box(&own);
    let from_uninstrumented = uninstrumented_1kb().await;
    black_box(&from_uninstrumented);
    instrumented_1kb().await;
}

#[tokio::main]
#[hotpath::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    uninstrumented_children_2kb().await;
    own_1kb_plus_uninstrumented_child_1kb().await;
    own_1kb_plus_uninstrumented_1kb_plus_instrumented_1kb().await;

    Ok(())
}
