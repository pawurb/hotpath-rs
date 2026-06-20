// Replicates the precise-channel receiver-closure scenario: a Sender clone is
// kept alive, every Receiver clone is dropped, and the report is taken *before*
// the senders are dropped. Crossbeam disconnects the channel on the last
// receiver drop, so the endpoint wrapper must emit `Closed` even though no
// `Sender` has been dropped yet.
//
// cargo run -p test-channels-crossbeam --example wrap_recv_clone_closed_crossbeam --features hotpath
fn main() {
    let guard = hotpath::HotpathGuardBuilder::new("main")
        .format(hotpath::Format::JsonPretty)
        .sections(vec![hotpath::Section::Channels])
        .build();

    let (tx, rx) = hotpath::channel!(
        crossbeam_channel::bounded::<i32>(10),
        wrap = true,
        label = "recv-clone-dropped"
    );

    let tx2 = tx.clone();
    let rx2 = rx.clone();

    tx.send(1).expect("Failed to send");

    // Drop both receiver clones. Only the last drop disconnects the channel.
    drop(rx);
    drop(rx2);

    // Both sender clones are still alive, yet the channel is already closed.
    assert!(
        tx.send(2).is_err(),
        "send should fail after all receivers dropped"
    );
    assert!(
        tx2.send(3).is_err(),
        "send should fail on the sender clone too"
    );

    // Report is generated while the senders are still alive.
    drop(guard);

    drop(tx);
    drop(tx2);

    println!("\nExample completed!");
}
