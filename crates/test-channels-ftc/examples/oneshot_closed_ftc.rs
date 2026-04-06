use smol::Timer;
use std::time::Duration;

#[allow(unused_mut)]
fn main() {
    smol::block_on(async {
        let _channels_guard = hotpath::HotpathGuardBuilder::new("main")
            .sections(vec![hotpath::Section::Channels])
            .build();

        let (tx, rx) = hotpath::channel!(
            futures_channel::oneshot::channel::<String>(),
            label = "oneshot-closed"
        );

        drop(rx);

        Timer::after(Duration::from_millis(50)).await;

        match tx.send("Hello oneshot!".to_string()) {
            Ok(_) => panic!("Not expected: send succeeded"),
            Err(_) => println!("Expected: Failed to send"),
        }
        Timer::after(Duration::from_millis(100)).await;

        println!("\nExample completed!");
    })
}
