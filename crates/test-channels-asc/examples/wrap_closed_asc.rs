// Dropping all receivers while a sender is still alive disconnects the channel.
// The endpoint wrapper must report the channel as `closed`, mirroring the proxy mode.
//
// cargo run -p test-channels-asc --example wrap_closed_asc --features hotpath
fn main() {
    smol::block_on(async {
        let guard = hotpath::HotpathGuardBuilder::new("main")
            .format(hotpath::Format::JsonPretty)
            .sections(vec![hotpath::Section::Channels])
            .build();

        let (tx, rx) = hotpath::channel!(
            async_channel::bounded::<i32>(10),
            wrap = true,
            label = "recv-dropped"
        );

        tx.send(1).await.expect("Failed to send");

        drop(rx);

        assert!(
            tx.send(2).await.is_err(),
            "send should fail after receiver dropped"
        );

        drop(guard);

        println!("\nExample completed!");
    })
}
