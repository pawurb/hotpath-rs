#[tokio::main]
async fn main() {
    let _channels_guard = hotpath::HotpathGuardBuilder::new("main")
        .with_sections(vec![hotpath::Section::Channels])
        .build();

    let (tx, rx) = hotpath::channel!(tokio::sync::oneshot::channel::<String>());

    drop(rx);

    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    match tx.send("Hello oneshot!".to_string()) {
        Ok(_) => panic!("Not expected: send succeeded"),
        Err(_) => println!("Expected: Failed to send"),
    }
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    println!("\nExample completed!");
}
