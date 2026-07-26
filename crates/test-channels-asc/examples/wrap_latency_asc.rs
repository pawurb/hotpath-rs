//! Run with:
//!   cargo run -p test-channels-asc --example wrap_latency_asc --features hotpath
// Demonstrates the wrap-channel processing-time histogram: each message is held in
// the channel for a known delay before being received, so the report's `proc_avg`
// and `proc_percentiles` reflect the exact send->receive latency. A proxy (non-wrap)
// channel is included to show it carries no latency histogram.
use std::time::Duration;

fn main() {
    smol::block_on(async {
        let guard = hotpath::HotpathGuardBuilder::new("main")
            .format(hotpath::Format::JsonPretty)
            .sections(vec![hotpath::Section::Channels])
            .percentiles(&[50.0, 95.0])
            .build();

        // Exact send->receive latency histogram.
        let (wtx, wrx) =
            hotpath::channel!(async_channel::unbounded::<i32>(), label = "wrap-latency");

        // proxy (forwarder): no latency histogram is recorded.
        let (ptx, prx) = hotpath::channel!(
            async_channel::unbounded::<i32>(),
            proxy = true,
            label = "proxy-latency"
        );

        for i in 0..10 {
            wtx.send(i).await.expect("Failed to send");
            ptx.send(i).await.expect("Failed to send");
        }

        // Hold messages so the recorded send->receive latency is dominated by this sleep.
        smol::Timer::after(Duration::from_millis(20)).await;

        let mut wrap_drained = 0;
        while wrx.try_recv().is_ok() {
            wrap_drained += 1;
        }
        let mut proxy_drained = 0;
        while prx.try_recv().is_ok() {
            proxy_drained += 1;
        }
        println!(
            "[main] drained wrap={} proxy={}",
            wrap_drained, proxy_drained
        );

        drop(guard);

        println!("\nExample completed!");
    })
}
