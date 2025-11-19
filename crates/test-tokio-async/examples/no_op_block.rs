#[tokio::main(flavor = "current_thread")]
#[cfg_attr(feature = "hotpath", hotpath::main)]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    hotpath::measure_block!("custom_block", {
        println!("custom_block output");
    });

    Ok(())
}
