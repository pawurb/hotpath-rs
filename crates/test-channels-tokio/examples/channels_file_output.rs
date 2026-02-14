#[tokio::main]
async fn main() {
    let _channels_guard = hotpath::HotpathGuardBuilder::new("main")
        .format(hotpath::Format::Json)
        .output_path("tmp/channels_output_test.json")
        .with_sections(vec![hotpath::Section::Channels])
        .build();

    let (tx, mut rx) = hotpath::channel!(
        tokio::sync::mpsc::channel::<i32>(10),
        label = "test-channel"
    );

    let sender_handle = tokio::spawn(async move {
        for i in 1..=5 {
            tx.send(i).await.expect("Failed to send");
        }
    });

    sender_handle.await.expect("Sender task failed");

    while let Some(msg) = rx.recv().await {
        println!("Received: {}", msg);
    }
}
