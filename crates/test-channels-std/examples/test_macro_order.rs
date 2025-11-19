use std::thread;
use std::time::Duration;

fn main() {
    #[cfg(feature = "hotpath")]
    let _channels_guard = hotpath::channels::ChannelsGuard::new();

    // Test: label first, then capacity
    let (tx1, rx1) = std::sync::mpsc::sync_channel::<i32>(10);
    #[cfg(feature = "hotpath")]
    let (tx1, rx1) = hotpath::channel!((tx1, rx1), label = "label-first", capacity = 10);

    // Test: capacity first, then label
    let (tx2, rx2) = std::sync::mpsc::sync_channel::<i32>(20);
    #[cfg(feature = "hotpath")]
    let (tx2, rx2) = hotpath::channel!((tx2, rx2), capacity = 20, label = "capacity-first");

    // Test: only label
    let (tx3, rx3) = std::sync::mpsc::channel::<i32>();
    #[cfg(feature = "hotpath")]
    let (tx3, rx3) = hotpath::channel!((tx3, rx3), label = "only-label");

    // Test: only capacity
    let (tx4, rx4) = std::sync::mpsc::sync_channel::<i32>(30);
    #[cfg(feature = "hotpath")]
    let (tx4, rx4) = hotpath::channel!((tx4, rx4), capacity = 30);

    thread::spawn(move || {
        tx1.send(1).unwrap();
        tx2.send(2).unwrap();
        tx3.send(3).unwrap();
        tx4.send(4).unwrap();
    });

    thread::sleep(Duration::from_millis(100));

    rx1.recv().unwrap();
    rx2.recv().unwrap();
    rx3.recv().unwrap();
    rx4.recv().unwrap();

    println!("All macro variations tested successfully!");
}
