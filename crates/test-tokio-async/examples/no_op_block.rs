//! Run with:
//!   cargo run -p test-tokio-async --example no_op_block --features hotpath

#[tokio::main(flavor = "current_thread")]
#[hotpath::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    hotpath::measure_block!("custom_block", {
        println!("custom_block output");
    });

    Ok(())
}
