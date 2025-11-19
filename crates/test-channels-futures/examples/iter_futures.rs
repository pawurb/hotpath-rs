use smol::Timer;
use std::time::Duration;

#[allow(dead_code)]
struct Actor {
    name: String,
}

#[allow(unused_mut)]
fn main() {
    smol::block_on(async {
        let _actor1 = Actor {
            name: "Actor 1".to_string(),
        };

        #[cfg(feature = "hotpath")]
        let _channels_guard = hotpath::channels::ChannelsGuard::new();

        println!("Creating channels in loops...\n");

        println!("Creating 3 unbounded channels:");
        for i in 0..3 {
            let (tx, mut rx) = futures_channel::mpsc::unbounded::<i32>();

            #[cfg(feature = "hotpath")]
            let (tx, mut rx) = hotpath::channel!((tx, rx), label = _actor1.name.clone());

            println!("  - Created unbounded channel {}", i);

            smol::spawn(async move {
                tx.unbounded_send(i).expect("Failed to send");
                let _ = rx.try_next();
            })
            .detach();
        }

        println!("\nCreating 3 bounded channels:");
        for i in 0..3 {
            let (mut tx, mut rx) = futures_channel::mpsc::channel::<i32>(10);

            #[cfg(feature = "hotpath")]
            let (mut tx, mut rx) = hotpath::channel!((tx, rx), capacity = 10, label = "bounded");

            println!("  - Created bounded channel {}", i);

            smol::spawn(async move {
                tx.try_send(i).expect("Failed to send");
                let _ = rx.try_next();
            })
            .detach();
        }

        println!("\nCreating 3 oneshot channels:");
        for i in 0..3 {
            let (tx, rx) = futures_channel::oneshot::channel::<String>();

            #[cfg(feature = "hotpath")]
            let (tx, rx) = hotpath::channel!((tx, rx));

            println!("  - Created oneshot channel {}", i);

            smol::spawn(async move {
                let _ = tx.send(format!("Message {}", i));
                let _ = rx.await;
            })
            .detach();
        }

        Timer::after(Duration::from_millis(500)).await;

        println!("\nAll channels created and used!");

        #[cfg(feature = "hotpath")]
        drop(_channels_guard);

        println!("\nExample completed!");
    })
}
