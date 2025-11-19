use std::thread;
use std::time::Duration;

#[allow(dead_code)]
struct Actor {
    name: String,
}

fn main() {
    let _actor1 = Actor {
        name: "Actor 1".to_string(),
    };

    #[cfg(feature = "hotpath")]
    let _channels_guard = hotpath::channels::ChannelsGuard::new();

    let (txa, _rxa) = std::sync::mpsc::channel::<i32>();
    #[cfg(feature = "hotpath")]
    let (txa, _rxa) = hotpath::channel!((txa, _rxa), label = "unbounded-channel");

    let (txb, rxb) = std::sync::mpsc::sync_channel::<i32>(10);
    #[cfg(feature = "hotpath")]
    let (txb, rxb) = hotpath::channel!((txb, rxb), capacity = 10, label = _actor1.name);

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
        println!("[Receiver] Received from bounded: {}", msg);
    }

    println!("\nStd channel example completed!");
}
