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

    let _channels_guard = hotpath::HotpathGuardBuilder::new("main")
        .sections(vec![hotpath::Section::Channels])
        .build();

    println!("Creating channels in loops...\n");

    println!("Creating 3 bounded channels:");
    for i in 0..3 {
        let (tx, mut rx) = hotpath::channel!(
            tokio::sync::mpsc::channel::<i32>(10),
            label = _actor1.name.clone()
        );

        println!("  - Created bounded channel {}", i);

        tokio::spawn(async move {
            let _ = tx.send(i).await;
            let _ = rx.recv().await;
        });
    }

    println!("\nCreating 3 unbounded channels:");
    for i in 0..3 {
        let (tx, mut rx) = hotpath::channel!(tokio::sync::mpsc::unbounded_channel::<i32>());

        println!("  - Created unbounded channel {}", i);

        tokio::spawn(async move {
            let _ = tx.send(i);
            let _ = rx.recv().await;
        });
    }

    println!("\nCreating 3 oneshot channels:");
    for i in 0..3 {
        let (tx, rx) = hotpath::channel!(tokio::sync::oneshot::channel::<String>());

        println!("  - Created oneshot channel {}", i);

        tokio::spawn(async move {
            let _ = tx.send(format!("Message {}", i));
            let _ = rx.await;
        });
    }

    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    println!("\nAll channels created and used!");

    drop(_channels_guard);

    println!("\nExample completed!");
}
