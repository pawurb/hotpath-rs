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

        let _channels_guard = hotpath::HotpathGuardBuilder::new("main")
            .with_sections(vec![hotpath::Section::Channels])
            .build();

        println!("Creating channels in loops...\n");

        println!("Creating 3 unbounded channels:");
        for i in 0..3 {
            let (tx, mut rx) = hotpath::channel!(async_channel::unbounded::<i32>());

            println!("  - Created unbounded channel {}", i);

            smol::spawn(async move {
                tx.send(i).await.expect("Failed to send");
                let _ = rx.recv().await;
            })
            .detach();
        }

        println!("\nCreating 3 bounded channels:");
        for i in 0..3 {
            let (mut tx, mut rx) = hotpath::channel!(async_channel::bounded::<i32>(10));

            println!("  - Created bounded channel {}", i);

            smol::spawn(async move {
                tx.send(i).await.expect("Failed to send");
                let _ = rx.recv().await;
            })
            .detach();
        }

        Timer::after(Duration::from_millis(500)).await;

        println!("\nAll channels created and used!");

        drop(_channels_guard);

        println!("\nExample completed!");
    })
}
