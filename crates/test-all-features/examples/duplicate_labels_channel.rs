//! Build-failure fixture: two `channel!` call sites sharing one literal label.
//! Expected to fail codegen with
//! ``symbol `hotpath: duplicate channel label "shared"` is already defined``.
//! The body is gated behind the `hotpath_dup_labels_fixture` cfg (not a
//! feature, so `--all-features` builds compile it to an empty program);
//! `tests/unique_labels.rs` enables it on purpose.
//!
//! Run with:
//!   cargo rustc -p test-all-features --example duplicate_labels_channel --features hotpath -- --cfg hotpath_dup_labels_fixture

#[cfg(hotpath_dup_labels_fixture)]
#[tokio::main(flavor = "current_thread")]
#[hotpath::main]
async fn main() {
    let (_tx_a, _rx_a) = hotpath::channel!(tokio::sync::mpsc::channel::<u8>(4), label = "shared");
    let (_tx_b, _rx_b) = hotpath::channel!(tokio::sync::mpsc::channel::<u8>(4), label = "shared");
}

#[cfg(not(hotpath_dup_labels_fixture))]
fn main() {}
