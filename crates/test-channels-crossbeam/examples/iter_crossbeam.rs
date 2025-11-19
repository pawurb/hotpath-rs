use std::thread;
use std::time::Duration;

fn main() {
    #[cfg(feature = "hotpath")]
    let _channels_guard = hotpath::channels::ChannelsGuard::new();

    println!("Creating channels in loops...\n");

    println!("Creating 3 unbounded channels:");
    let mut handles = vec![];
    for i in 0..3 {
        let (tx, rx) = crossbeam_channel::unbounded::<i32>();

        #[cfg(feature = "hotpath")]
        let (tx, rx) = hotpath::channel!((tx, rx));

        println!("  - Created unbounded channel {}", i);

        let handle = thread::spawn(move || {
            tx.send(i).expect("Failed to send");
            rx.recv().expect("Failed to recv");
        });
        handles.push(handle);
    }

    println!("\nCreating 3 bounded channels:");
    for i in 0..3 {
        let (tx, rx) = crossbeam_channel::bounded::<i32>(10);

        #[cfg(feature = "hotpath")]
        let (tx, rx) = hotpath::channel!((tx, rx), capacity = 10, label = "bounded");

        println!("  - Created bounded channel {}", i);

        let handle = thread::spawn(move || {
            tx.send(i).expect("Failed to send");
            rx.recv().expect("Failed to recv");
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.join().unwrap();
    }

    thread::sleep(Duration::from_millis(100));

    println!("\nAll channels created and used!");

    #[cfg(feature = "hotpath")]
    drop(_channels_guard);

    println!("\nExample completed!");
}
