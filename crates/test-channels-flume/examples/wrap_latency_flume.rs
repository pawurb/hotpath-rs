// Demonstrates the wrap-channel processing-time histogram: each message is held in
// the channel for a known delay before being received, so the report's `proc_avg`
// and `proc_percentiles` reflect the exact send->receive latency. A proxy (non-wrap)
// channel is included to show it carries no latency histogram.
//
// cargo run -p test-channels-flume --example wrap_latency_flume --features hotpath
use std::thread;
use std::time::Duration;

fn main() {
    let guard = hotpath::HotpathGuardBuilder::new("main")
        .format(hotpath::Format::JsonPretty)
        .sections(vec![hotpath::Section::Channels])
        .percentiles(&[50.0, 95.0])
        .build();

    // wrap = true: exact send->receive latency histogram.
    let (wtx, wrx) = hotpath::channel!(
        flume::unbounded::<i32>(),
        wrap = true,
        label = "wrap-latency"
    );

    // proxy (no wrap): no latency histogram is recorded.
    let (ptx, prx) = hotpath::channel!(flume::unbounded::<i32>(), label = "proxy-latency");

    for i in 0..10 {
        wtx.send(i).expect("Failed to send");
        ptx.send(i).expect("Failed to send");
    }

    // Hold messages so the recorded send->receive latency is dominated by this sleep.
    thread::sleep(Duration::from_millis(20));

    let wrap_drained: Vec<i32> = wrx.try_iter().collect();
    let proxy_drained: Vec<i32> = prx.try_iter().collect();
    println!(
        "[main] drained wrap={} proxy={}",
        wrap_drained.len(),
        proxy_drained.len()
    );

    drop(guard);

    println!("\nExample completed!");
}
