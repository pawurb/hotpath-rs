#[allow(unused_mut)]
fn main() {
    let _channels_guard = hotpath::HotpathGuardBuilder::new("main")
        .with_sections(vec![hotpath::Section::Channels])
        .build();

    let (tx1, rx1) = hotpath::channel!(
        crossbeam_channel::bounded::<i32>(5),
        label = "closed-sender",
        capacity = 5
    );

    let (tx2, rx2) = hotpath::channel!(
        crossbeam_channel::bounded::<i32>(5),
        label = "closed-receiver",
        capacity = 5
    );

    let (tx3, rx3) = hotpath::channel!(
        crossbeam_channel::unbounded::<i32>(),
        label = "closed-unbounded"
    );

    drop(tx1);

    // Try to receive from closed sender
    match rx1.recv() {
        Ok(msg) => println!("[Closed Sender] Received: {}", msg),
        Err(_) => println!("[Closed Sender] Channel closed"),
    }

    // Drop receiver immediately
    drop(rx2);

    // Try to send to closed receiver
    match tx2.send(42) {
        Ok(_) => println!("[Closed Receiver] Sent message"),
        Err(_) => println!("[Closed Receiver] Channel closed"),
    }

    // Drop unbounded sender
    drop(tx3);

    // Try to receive from closed unbounded
    match rx3.recv() {
        Ok(msg) => println!("[Closed Unbounded] Received: {}", msg),
        Err(_) => println!("[Closed Unbounded] Channel closed"),
    }

    std::thread::sleep(std::time::Duration::from_millis(100));

    drop(_channels_guard);

    println!("\nExample completed!");
}
