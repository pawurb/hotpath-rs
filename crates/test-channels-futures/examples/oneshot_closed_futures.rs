use smol::Timer;
use std::time::Duration;

#[allow(unused_mut)]
fn main() {
    smol::block_on(async {
        #[cfg(feature = "hotpath")]
        let _channels_guard = hotpath::channels::ChannelsGuard::new();

        let (tx, rx) = futures_channel::oneshot::channel::<String>();
        #[cfg(feature = "hotpath")]
        let (tx, rx) = hotpath::channel!((tx, rx), label = "oneshot-closed");

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
