//! Build-failure fixture: `#[measure(label)]` and `measure_block!` share the
//! functions namespace, so one literal label on both is rejected. Expected to
//! fail codegen with
//! ``symbol `hotpath: duplicate function label "shared"` is already defined``.
//! Gated behind `dup-labels-fixture` so workspace-wide example builds skip it;
//! `tests/unique_labels.rs` builds it on purpose.
//!
//! Run with:
//!   cargo build -p test-all-features --example duplicate_labels_function --features hotpath,dup-labels-fixture

#[hotpath::measure(label = "shared")]
fn measured() {}

#[hotpath::main]
fn main() {
    measured();
    hotpath::measure_block!("shared", {
        std::hint::black_box(0);
    });
}
