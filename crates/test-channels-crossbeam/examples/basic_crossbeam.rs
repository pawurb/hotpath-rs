#[allow(unused_mut)]
fn main() {
    let _channels_guard = hotpath::HotpathGuardBuilder::new("main")
        .with_sections(vec![hotpath::Section::Channels])
        .build();

    let (txa, _rxa) = hotpath::channel!(crossbeam_channel::unbounded::<i32>(), log = true);

    let (txb, rxb) = hotpath::channel!(crossbeam_channel::bounded::<i32>(10), capacity = 10);

    let (txc, rxc) = hotpath::channel!(
        crossbeam_channel::bounded::<String>(1),
        label = "hello-there",
        capacity = 1
    );

    let sender_handle = std::thread::spawn(move || {
        for i in 1..=3 {
            println!("[Sender] Sending message: {}", i);
            txa.send(i).expect("Failed to send");
            std::thread::sleep(std::time::Duration::from_millis(100));
        }

        for i in 1..=3 {
            println!("[Sender] Sending message: {}", i);
            txb.send(i).expect("Failed to send");
            std::thread::sleep(std::time::Duration::from_millis(250));
        }

        println!("[Sender] Done sending messages");
    });

    let bounded_receiver_handle = std::thread::spawn(move || match rxc.recv() {
        Ok(msg) => println!("[Bounded-1] Received: {}", msg),
        Err(_) => println!("[Bounded-1] Sender dropped"),
    });

    println!("[Bounded-1] Sending message");
    txc.send("Hello from bounded channel!".to_string())
        .expect("Failed to send");

    sender_handle.join().expect("Sender thread failed");
    bounded_receiver_handle
        .join()
        .expect("Bounded receiver thread failed");

    drop(_channels_guard);

    while let Ok(msg) = rxb.recv() {
        println!("[Receiver] Received message: {}", msg);
    }

    if let Ok(secs) = std::env::var("TEST_SLEEP_SECONDS") {
        if let Ok(duration) = secs.parse::<u64>() {
            std::thread::sleep(std::time::Duration::from_secs(duration));
        }
    }

    println!("\nExample completed!");
}
