use std::thread;
use std::time::Duration;

#[allow(unused_mut)]
fn main() {
    let _channels_guard = hotpath::HotpathGuardBuilder::new("main")
        .sections(vec![hotpath::Section::Channels])
        .build();

    let (txa, rxa) = hotpath::channel!(std::sync::mpsc::channel::<i32>(), label = "unbounded");

    let (txb, rxb) = hotpath::channel!(
        std::sync::mpsc::sync_channel::<i32>(10),
        label = "bounded",
        capacity = 10
    );

    println!("[Unbounded] Sending 3 messages...");
    for i in 1..=3 {
        txa.send(i).expect("Failed to send");
    }

    for _ in 0..3 {
        if let Ok(msg) = rxa.recv() {
            println!("[Unbounded] Received: {}", msg);
        }
    }

    println!("[Bounded] Sending 3 messages...");
    for i in 1..=3 {
        txb.send(i).expect("Failed to send");
    }

    for _ in 0..3 {
        if let Ok(msg) = rxb.recv() {
            println!("[Bounded] Received: {}", msg);
        }
    }

    println!("\nClosing channels from receiver side...");

    drop(rxa);
    println!("[Unbounded] Receiver closed");

    drop(rxb);
    println!("[Bounded] Receiver closed");

    thread::sleep(Duration::from_millis(100));

    println!("\nAttempting to send after closing receivers...");

    match txa.send(999) {
        Ok(_) => println!("[Unbounded] Send succeeded (buffered, receiver already closed)"),
        Err(_) => println!("[Unbounded] Send failed - channel closed"),
    }

    match txb.send(999) {
        Ok(_) => println!("[Bounded] Send succeeded (unexpected)"),
        Err(_) => println!("[Bounded] Send failed - channel closed"),
    }

    thread::sleep(Duration::from_millis(100));

    println!("\nExample completed!");
}
