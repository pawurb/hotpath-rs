//! Run with:
//!   cargo run -p test-channels-std --example wrap_arg_orders_std --features hotpath
// Exercises bounded-std channels with `capacity` in every argument
// position, combined with `label` and `log` in every order. Each channel gets a distinct
// label so the report lists them all.
use std::sync::mpsc;

fn main() {
    let guard = hotpath::HotpathGuardBuilder::new("main")
        .format(hotpath::Format::JsonPretty)
        .sections(vec![hotpath::Section::Channels])
        .build();

    let chans = [
        hotpath::channel!(mpsc::sync_channel::<i32>(4), capacity = 4, label = "a"),
        hotpath::channel!(mpsc::sync_channel::<i32>(4), label = "b", capacity = 4),
        hotpath::channel!(mpsc::sync_channel::<i32>(4), capacity = 4, label = "c"),
        hotpath::channel!(mpsc::sync_channel::<i32>(4), label = "d", capacity = 4),
        hotpath::channel!(mpsc::sync_channel::<i32>(4), capacity = 4, label = "e"),
        hotpath::channel!(mpsc::sync_channel::<i32>(4), label = "f", capacity = 4),
    ];

    // log = true combined with capacity and label, both orders.
    let (gtx, grx) = hotpath::channel!(
        mpsc::sync_channel::<i32>(4),
        label = "g",
        capacity = 4,
        log = true
    );
    let (htx, hrx) = hotpath::channel!(
        mpsc::sync_channel::<i32>(4),
        log = true,
        capacity = 4,
        label = "h"
    );

    for (tx, rx) in &chans {
        tx.send(1).unwrap();
        rx.recv().unwrap();
    }
    gtx.send(1).unwrap();
    grx.recv().unwrap();
    htx.send(1).unwrap();
    hrx.recv().unwrap();

    drop(guard);
    println!("\nExample completed!");
}
