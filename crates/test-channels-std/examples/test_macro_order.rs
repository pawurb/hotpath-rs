use std::thread;
use std::time::Duration;

fn main() {
    let _channels_guard = hotpath::HotpathGuardBuilder::new("main")
        .sections(vec![hotpath::Section::Channels])
        .build();

    // Test: label first, then capacity
    let (tx1, rx1) = hotpath::channel!(
        std::sync::mpsc::sync_channel::<i32>(10),
        label = "label-first",
        capacity = 10
    );

    // Test: capacity first, then label
    let (tx2, rx2) = hotpath::channel!(
        std::sync::mpsc::sync_channel::<i32>(20),
        capacity = 20,
        label = "capacity-first"
    );

    // Test: only label
    let (tx3, rx3) = hotpath::channel!(std::sync::mpsc::channel::<i32>(), label = "only-label");

    // Test: only capacity
    let (tx4, rx4) = hotpath::channel!(std::sync::mpsc::sync_channel::<i32>(30), capacity = 30);

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
