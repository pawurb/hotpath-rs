// Drop-in parity check: binding the wrapped endpoints to their
// `hotpath::wrap::std::sync::mpsc` types must compile identically whether the
// `hotpath` feature is on (instrumented wrappers) or off (raw std aliases).
//
// cargo run -p test-channels-std --example wrap_dropin_std
// cargo run -p test-channels-std --example wrap_dropin_std --features hotpath
use hotpath::wrap::std::sync::mpsc::{Receiver, SyncSender};
use std::sync::mpsc;

fn main() {
    let (tx, rx): (SyncSender<i32>, Receiver<i32>) =
        hotpath::channel!(mpsc::sync_channel::<i32>(8), wrap = true, capacity = 8);

    tx.send(1).expect("send");
    let got = rx.recv().expect("recv");
    println!("got {got}");
}
