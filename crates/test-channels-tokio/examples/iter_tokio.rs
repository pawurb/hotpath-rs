#[allow(dead_code)]
struct Actor {
    name: String,
}

#[allow(unused_mut)]
#[tokio::main]
async fn main() {
    let _actor1 = Actor {
        name: "Actor 1".to_string(),
    };

    #[cfg(feature = "hotpath")]
    let _channels_guard = hotpath::channels::ChannelsGuard::new();

    println!("Creating channels in loops...\n");

    println!("Creating 3 bounded channels:");
    for i in 0..3 {
        let (tx, mut rx) = tokio::sync::mpsc::channel::<i32>(10);

        #[cfg(feature = "hotpath")]
        let (tx, mut rx) = hotpath::channel!((tx, rx), label = _actor1.name.clone());

        println!("  - Created bounded channel {}", i);

        tokio::spawn(async move {
            let _ = tx.send(i).await;
            let _ = rx.recv().await;
        });
    }

    println!("\nCreating 3 unbounded channels:");
    for i in 0..3 {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<i32>();

        #[cfg(feature = "hotpath")]
        let (tx, mut rx) = hotpath::channel!((tx, rx));

        println!("  - Created unbounded channel {}", i);

        tokio::spawn(async move {
            let _ = tx.send(i);
            let _ = rx.recv().await;
        });
    }

    println!("\nCreating 3 oneshot channels:");
    for i in 0..3 {
        let (tx, rx) = tokio::sync::oneshot::channel::<String>();

        #[cfg(feature = "hotpath")]
        let (tx, rx) = hotpath::channel!((tx, rx));

        println!("  - Created oneshot channel {}", i);

        tokio::spawn(async move {
            let _ = tx.send(format!("Message {}", i));
            let _ = rx.await;
        });
    }

    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    println!("\nAll channels created and used!");

    #[cfg(feature = "hotpath")]
    drop(_channels_guard);

    println!("\nExample completed!");
}
