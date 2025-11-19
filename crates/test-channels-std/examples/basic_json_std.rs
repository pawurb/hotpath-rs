use std::thread;
use std::time::Duration;

#[allow(unused_mut)]
fn main() {
    #[cfg(feature = "hotpath")]
    let _channels_guard =
        hotpath::channels::ChannelsGuard::new().format(hotpath::channels::Format::JsonPretty);

    let (txa, mut _rxa) = std::sync::mpsc::channel::<i32>();
    #[cfg(feature = "hotpath")]
    let (txa, mut _rxa) = hotpath::channel!((txa, _rxa), label = "unbounded");

    let (txb, rxb) = std::sync::mpsc::sync_channel::<i32>(10);
    #[cfg(feature = "hotpath")]
    let (txb, rxb) = hotpath::channel!((txb, rxb), label = "bounded", capacity = 10);

    let sender_handle = thread::spawn(move || {
        for i in 1..=3 {
            println!("[Sender] Sending to unbounded: {}", i);
            txa.send(i).expect("Failed to send");
            thread::sleep(Duration::from_millis(100));
        }

        for i in 1..=3 {
            println!("[Sender] Sending to bounded: {}", i);
            txb.send(i).expect("Failed to send");
            thread::sleep(Duration::from_millis(250));
        }

        println!("[Sender] Done sending messages");
    });

    sender_handle.join().unwrap();

    for msg in rxb.iter() {
        println!("[Receiver] Received message: {}", msg);
    }

    println!("\nExample completed!");
}
