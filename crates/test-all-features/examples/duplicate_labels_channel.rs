//! Build-failure fixture: two `channel!` call sites sharing one literal label.
//! Expected to fail codegen with
//! ``symbol `hotpath: duplicate channel label "shared"` is already defined``.
//! Gated behind `dup-labels-fixture` so workspace-wide example builds skip it;
//! `tests/unique_labels.rs` builds it on purpose.
//!
//! Run with:
//!   cargo build -p test-all-features --example duplicate_labels_channel --features hotpath,dup-labels-fixture

#[tokio::main(flavor = "current_thread")]
#[hotpath::main]
async fn main() {
    let (_tx_a, _rx_a) = hotpath::channel!(tokio::sync::mpsc::channel::<u8>(4), label = "shared");
    let (_tx_b, _rx_b) = hotpath::channel!(tokio::sync::mpsc::channel::<u8>(4), label = "shared");
}
